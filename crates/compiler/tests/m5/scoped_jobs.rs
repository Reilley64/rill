use super::*;
use compiler::analysis::ownership::{DependencyContract, GraphObligation, analyze_ownership};
use compiler::analysis::typed::{
    CollectionOperation, CollectionStorage, OwnershipCoverage, ResultUseBlocker, TypedCompletion,
};
use compiler::analysis::{QueryError, Statistics, scalars::ScalarFindingKind};

const TWO_ARROW_BODY: &str = r#"
    function runTwo<W, R>(jobs: W): R;
    implement<F, G, A, B> runTwo<[F, G], [A, B]>(jobs: [F, G]): [A, B]
    where callableOnce(F, [], A), callableOnce(G, [], B), sendable(F), sendable(G), sendable(A), sendable(B) {
        return [jobs[0](), jobs[1]()];
    }
"#;

#[test]
fn unsupported_list_arrow_shapes_keep_the_old_caps() {
    let source = format!(
        "{TWO_ARROW_BODY}{}",
        r#"
        function partial(xs: List<i32>): [i32, i32] {
            return runTwo([() => (&xs)[0u64], () => 5i32]);
        }
        function mixed(xs: List<i32>): [[bool, i32], i32] {
            return runTwo([(): [bool, i32] => [true, 2i32], () => (&xs)[1u64]]);
        }
    "#
    );
    probe(
        "scoped_jobs::unsupported_list_arrow_shapes_keep_the_old_caps",
        &source,
        |request, module, receipt| {
            // Measured on b887bdd: M4 still checks each actual Result residence,
            // but there is no pair of List reads to retain or lower in M5.
            let graph = loader::load_installed(request.clone()).unwrap();
            let typed = compiler::analysis::typed::analyze_typed(
                &graph,
                AnalysisLimits {
                    max_work: 443898,
                    max_entries: 55405,
                    max_depth: 12,
                },
            );
            assert!(typed.receipt().is_some());
            for (name, max_work, max_entries) in [("partial", 2378, 148), ("mixed", 2383, 152)] {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report = lower_body(
                    receipt,
                    module,
                    &d.id,
                    AnalysisLimits {
                        max_work,
                        max_entries,
                        max_depth: 3,
                    },
                )
                .unwrap();
                assert_eq!(report.completion(), Completion::Incomplete);
                assert!(report.mir().is_none() && report.findings().is_empty());
            }
        },
    );
}

#[test]
fn installed_two_list_arrow_frames() {
    use compiler::analysis::generics::{GoalState, callables::CaptureForm};
    use compiler::analysis::ownership::GraphReport;

    let source = format!(
        "{TWO_ARROW_BODY}{}",
        r#"
        import helper::work as join;
        function sameRoot(input: List<i32>): [i32, i32] {
            let numbers = input;
            let [a, b] = runTwo([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
            dispose(numbers);
            return [a, b];
        }
        function swapped(left: List<i32>, right: List<i32>): [i32, i32] {
            let [a, b] = join([(() => { return (&right)[(1u64)]; }), () => (&left)[0u64]]);
            return [a, b];
        }
        function conflicting(input: List<i32>): [i32, i32] {
            let mut numbers = input;
            let exclusive = &mut numbers;
            let [a, b] = runTwo([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
            let later = exclusive;
            return [a, b];
        }
        function unknown(input: List<i32>): [i32, i32] {
            return runTwo([() => (&input)[0u64], () => 5i32]);
        }
        function deep(input: List<i32>): [i32, i32] {
            return runTwo((((([((((() => (&input)[0u64])))), () => (&input)[1u64]])))));
        }
        "#,
    );
    let helper = TWO_ARROW_BODY
        .replace("runTwo", "work")
        .replace("function work", "export function work");
    probe_report_with_modules(
        "scoped_jobs::installed_two_list_arrow_frames",
        &source,
        &[("helper.rill", &helper)],
        |request, module, typed| {
            let receipt = typed
                .receipt()
                .unwrap_or_else(|| panic!("{:?}", typed.result_use()));
            let decl = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name| {
                lower_body(&receipt, module, &decl(name).id, AnalysisLimits::default()).unwrap()
            };
            let graph = analyze_ownership(&receipt, AnalysisLimits::default());
            let root = |graph: &GraphReport<'_>, name| {
                graph
                    .roots()
                    .iter()
                    .position(|r| {
                        r.site().module == &module.key && r.site().node == decl(name).node
                    })
                    .unwrap()
            };
            for (name, expected_backings, expected_selectors, expected_module) in [
                ("sameRoot", ["numbers", "numbers"], [0, 1], "main.rill"),
                (
                    "swapped",
                    ["right: List<i32>", "left: List<i32>"],
                    [1, 0],
                    "helper.rill",
                ),
            ] {
                let local = lower(name);
                let reached = &graph.roots()[root(&graph, name)];
                assert_eq!(
                    local.completion(),
                    Completion::Checked,
                    "{name}: {:?}",
                    local.blocked_at()
                );
                assert_eq!(
                    reached.completion(),
                    Completion::Checked,
                    "{name}: {:?}",
                    reached.blocked_at()
                );
                for mir in [local.mir().unwrap(), reached.mir().unwrap()] {
                    assert!(mir.cancellation().is_none());
                    let path = straight_blocks(mir, mir.entry);
                    let (call_pc, block) = path
                        .iter()
                        .enumerate()
                        .find(|(_, block)| {
                            matches!(block.operation, Operation::CallListArrows { .. })
                        })
                        .unwrap();
                    let Operation::CallListArrows {
                        implementation,
                        body,
                        slots,
                    } = &block.operation
                    else {
                        unreachable!()
                    };
                    let ordinary = typed
                        .bodies()
                        .ordinary_call(module, block.site.node)
                        .unwrap()
                        .unwrap();
                    assert!(ordinary.checked && ordinary.premise.is_none());
                    let goal = typed
                        .bodies()
                        .goal(ordinary.goal.as_ref().unwrap())
                        .unwrap();
                    assert_eq!(goal.state, GoalState::Proved);
                    assert!(std::ptr::eq(
                        goal.selection.as_ref().unwrap().implementation,
                        *implementation
                    ));
                    assert_eq!(implementation.module.origin, Origin::Application);
                    assert_eq!(implementation.module.path, expected_module);
                    assert_eq!(
                        body.span.bytes(),
                        b"{\n        return [jobs[0](), jobs[1]()];\n    }"
                    );
                    assert_ne!(slots[0].origin.node, slots[1].origin.node);
                    assert!(!std::ptr::eq(
                        slots[0].body.as_ref(),
                        slots[1].body.as_ref()
                    ));
                    let frames = goal
                        .dependencies
                        .iter()
                        .filter_map(|id| {
                            let dep = typed.bodies().goal(id).unwrap();
                            assert_eq!(dep.state, GoalState::Proved);
                            assert!(dep.premise.is_none());
                            dep.arrow_origin.as_deref()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(frames.len(), 2);
                    let mut acquisition_pc = Vec::new();
                    for (i, slot) in slots.iter().enumerate() {
                        assert_eq!(slot.origin.node, frames[i].site.node);
                        let [capture] = frames[i].captures.as_slice() else {
                            panic!("singleton proved capture")
                        };
                        assert_eq!(capture.form, CaptureForm::ExplicitBorrow { mutable: false });
                        assert_eq!(slot.capture.root.node, capture.site.node);
                        assert_eq!(
                            slot.backing.root.span.bytes(),
                            expected_backings[i].as_bytes()
                        );
                        assert_eq!(
                            slot.invocation.span.bytes(),
                            [b"jobs[0]()", b"jobs[1]()"][i]
                        );
                        assert_eq!(slot.result.tuple_element(), Some(i));
                        let before_call = &path[..call_pc];
                        let borrow = before_call.iter().position(|block| matches!(block.operation, Operation::Borrow { source, holder, exclusive: false }
                            if source.root.node == slot.backing.root.node && holder.root.node == slot.capture.root.node)).unwrap();
                        let formation = before_call.iter().position(|block| matches!(block.operation, Operation::FormClosure { closure, capture, .. }
                            if closure.node == slot.environment.root.node && capture.node == slot.capture.root.node)).unwrap();
                        assert!(borrow < formation);
                        acquisition_pc.push((borrow, formation));
                        let child = straight_blocks(&slot.body, slot.body.entry);
                        let [read, export, cleanup, end] = child.as_slice() else {
                            panic!("read, transfer scalar, end child view, return")
                        };
                        let Operation::ReadListI32 {
                            view,
                            backing,
                            index,
                            into: Some(output),
                            ..
                        } = read.operation
                        else {
                            panic!("checked child read")
                        };
                        assert_eq!(view.root.node, slot.capture.root.node);
                        assert_eq!(backing.root.node, slot.incoming_backing.root.node);
                        assert_eq!(index, expected_selectors[i]);
                        let Operation::Cleanup(items) = &cleanup.operation else {
                            panic!("one child cleanup")
                        };
                        assert!(
                            matches!(items.as_slice(), [Obligation::EndStorage(place)] if place.root.node == slot.capture.root.node)
                        );
                        assert!(
                            matches!(export.operation, Operation::Export(place) if place.root.node == output.root.node)
                        );
                        assert!(matches!(end.terminator, Terminator::Return));
                        assert!(slot.body.cancellation().is_none());
                        assert!(!path.iter().any(|b| matches!(&b.operation, Operation::Cleanup(items) if items.iter().any(|item| matches!(item, Obligation::EndStorage(place) if [slot.capture.root.node,slot.environment.root.node].contains(&place.root.node))))));
                    }
                    assert!(acquisition_pc[0].1 < acquisition_pc[1].0);
                    assert!(path[call_pc + 1..].iter().any(|block| matches!(&block.operation, Operation::Cleanup(items)
                        if items.iter().any(|item| matches!(item, Obligation::DisposeListI32 { operand, .. }
                            if operand.root.node == slots[0].backing.root.node)))));
                    assert!(reached.dependencies().iter().any(|edge| std::ptr::eq(
                        edge.declaration,
                        *implementation
                    ) && edge.contract
                        == DependencyContract::SynchronousListArrows {
                            body: body.node,
                            arrow_bodies: [slots[0].body_site.node, slots[1].body_site.node],
                        }));
                }
                if name == "sameRoot" {
                    check_exact(&receipt, module, &decl(name).id, local.statistics());
                }
            }
            let deep = lower("deep");
            assert!(deep.mir().is_some());
            let max_depth = lower("sameRoot").statistics().max_depth;
            assert!(deep.statistics().max_depth > max_depth);
            let limited = lower_body(
                &receipt,
                module,
                &decl("deep").id,
                AnalysisLimits {
                    max_depth,
                    ..Default::default()
                },
            )
            .unwrap();
            crate::limit::assert_body_limit(&limited);
            let conflict = lower("conflicting");
            let [finding] = conflict.findings() else {
                panic!("{:?} at {:?}", conflict.completion(), conflict.blocked_at())
            };
            assert_eq!(conflict.completion(), Completion::SourceRejected);
            assert!(conflict.mir().is_none());
            assert_eq!(finding.code(), "R0201");
            assert_eq!(finding.site.span.bytes(), b"&numbers");
            assert_eq!(finding.related.span.bytes(), b"&mut numbers");
            assert_eq!(
                graph.roots()[root(&graph, "conflicting")].completion(),
                Completion::SourceRejected
            );
            // An eligible neighbor must not make a missed specialization more expensive.
            let unknown = lower_body(
                &receipt,
                module,
                &decl("unknown").id,
                AnalysisLimits {
                    max_work: 2622,
                    max_entries: 253,
                    max_depth: 3,
                },
            )
            .unwrap();
            assert_eq!(unknown.completion(), Completion::Incomplete);
            assert!(unknown.mir().is_none());
            assert!(graph.roots()[root(&graph, "unknown")].mir().is_none());
            assert!(
                graph
                    .open_obligations()
                    .contains(&GraphObligation::MaskingAndJoin)
            );

            let Statistics {
                work,
                entries,
                max_depth,
            } = graph.statistics();
            let exact = AnalysisLimits {
                max_work: work,
                max_entries: entries,
                max_depth,
            };
            let exact_graph = analyze_ownership(&receipt, exact);
            assert!(
                exact_graph.roots()[root(&exact_graph, "sameRoot")]
                    .mir()
                    .is_some()
            );
            for caps in [
                AnalysisLimits {
                    max_work: work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: max_depth - 1,
                    ..exact
                },
            ] {
                let limited = analyze_ownership(&receipt, caps);
                crate::limit::assert_graph_limit(&limited);
            }
            let foreign = loader::load_installed(request.clone()).unwrap();
            let foreign_module = foreign
                .modules()
                .iter()
                .find(|m| m.key == module.key)
                .unwrap();
            let foreign_id = &foreign_module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("sameRoot"))
                .unwrap()
                .id;
            for (module, id) in [(module, foreign_id), (foreign_module, &decl("sameRoot").id)] {
                assert_eq!(
                    lower_body(&receipt, module, id, AnalysisLimits::default()).unwrap_err(),
                    QueryError::InvalidRequest
                );
            }
        },
    );
}

#[test]
fn shared_job_source_moves_and_releases_through_the_installed_list_backing() {
    let mut observed = Vec::new();
    probe_with_typed(
        "scoped_jobs::shared_job_source_moves_and_releases_through_the_installed_list_backing",
        r#"
            import std::core::dispose as release;
            function explicit(input: List<i32>): void {
                let numbers = input;
                {
                    let first = &numbers;
                    let second = &numbers;
                    let lastFirst = first;
                    let lastSecond = second;
                }
                release(numbers);
            }
            function implicit(input: List<i32>, explicitExit: bool): void {
                let numbers = input;
                if (explicitExit) { return; }
            }
            function reuse(input: List<i32>): void {
                let numbers = input;
                release(numbers);
                release(numbers);
            }
            function premature(input: List<i32>): void {
                let numbers = input;
                let first = &numbers;
                let second = &numbers;
                let lastFirst = first;
                release(numbers);
                let late = second;
            }
        "#,
        |request, module, receipt, typed| {
            let decl = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name| {
                lower_body(receipt, module, &decl(name).id, AnalysisLimits::default()).unwrap()
            };
            let explicit = lower("explicit");
            let implicit = lower("implicit");
            let reuse = lower("reuse");
            let premature = lower("premature");
            let verdicts = [
                explicit.completion(),
                implicit.completion(),
                reuse.completion(),
                premature.completion(),
            ];
            eprintln!(
                "test_mode={}: {verdicts:?}; first blocker {:?}",
                request.test_mode,
                explicit
                    .blocked_at()
                    .map(|site| site.span.bytes().escape_ascii().to_string())
            );
            observed.push(verdicts);
            let (Some(explicit_mir), Some(implicit_mir)) = (explicit.mir(), implicit.mir()) else {
                return;
            };
            let [finding] = reuse.findings() else {
                panic!("one reuse finding")
            };
            assert!(reuse.mir().is_none());
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.related.span.bytes(), b"release(numbers)");
            assert!(finding.site.span.start() > finding.related.span.start());
            let [finding] = premature.findings() else {
                panic!("one live-backing release finding")
            };
            assert!(premature.mir().is_none());
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"release(numbers)");
            assert_eq!(finding.related.span.bytes(), b"&numbers");
            check_exact(receipt, module, &decl("explicit").id, explicit.statistics());
            let list = typed.list_role().unwrap();
            let cleanup = typed
                .collection_protection()
                .find(|p| p.operation == CollectionOperation::DisposeList)
                .unwrap();
            assert!(std::ptr::eq(cleanup.owner, list.declaration));
            assert!(std::ptr::eq(cleanup.family, typed.dispose_role().unwrap()));
            assert_eq!(cleanup.coverage, OwnershipCoverage::Clear);
            assert_eq!(
                *cleanup.provider.unwrap().artifact.context(),
                receipt.context()
            );
            assert!(
                typed
                    .collection_representations()
                    .any(|r| r.storage == CollectionStorage::Elements
                        && std::ptr::eq(r.declaration, list.declaration)
                        && *r.artifact.context() == receipt.context())
            );
            let provider = cleanup.provider.unwrap();
            let check_cleanup = |mir: &Mir<'_>, entry| {
                let drops = path_drops(mir, entry);
                let [
                    Obligation::DisposeListI32 {
                        operand,
                        implementation,
                        bridge,
                    },
                ] = drops.as_slice()
                else {
                    panic!("one backing cleanup and no element drop")
                };
                assert_eq!(operand.root.span.bytes(), b"numbers");
                assert!(std::ptr::eq(*implementation, &provider.implementation.id));
                assert!(std::ptr::eq(*bridge, &provider.bridge.id));
                operand.root.node
            };
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            assert!(
                graph
                    .open_obligations()
                    .contains(&GraphObligation::MaskingAndJoin)
            );
            let root = |name| {
                graph
                    .roots()
                    .iter()
                    .find(|root| {
                        root.site().module == &module.key && root.site().node == decl(name).node
                    })
                    .unwrap()
            };
            assert_eq!(root("reuse").completion(), Completion::SourceRejected);
            assert!(root("reuse").mir().is_none());
            assert_eq!(root("premature").completion(), Completion::SourceRejected);
            assert!(root("premature").mir().is_none());
            assert_eq!(root("explicit").completion(), Completion::Checked);
            let [edge] = root("explicit").dependencies() else {
                panic!("one installed disposal edge")
            };
            assert!(std::ptr::eq(edge.declaration, &provider.implementation.id));
            assert_eq!(
                edge.contract,
                DependencyContract::ListI32Disposal {
                    bridge: &provider.bridge.id
                }
            );
            for mir in [explicit_mir, root("explicit").mir().unwrap()] {
                let owner = check_cleanup(mir, mir.entry);
                let path = straight_blocks(mir, mir.entry);
                let actions: Vec<_> = path
                    .iter()
                    .filter_map(|block| match &block.operation {
                        Operation::Move {
                            from,
                            into: Some(into),
                        } if into.root.node == owner => Some(from.root.span.bytes()),
                        Operation::Borrow {
                            source,
                            holder,
                            exclusive: false,
                        } if source.root.node == owner => Some(holder.root.span.bytes()),
                        Operation::Cleanup(items)
                            if items
                                .iter()
                                .any(|item| matches!(item, Obligation::DisposeListI32 { .. })) =>
                        {
                            Some(block.site.span.bytes())
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    actions,
                    [
                        b"input: List<i32>".as_slice(),
                        b"first",
                        b"second",
                        b"release(numbers)"
                    ]
                );
                assert!(mir.cancellation().is_none());
            }
            for mir in [implicit_mir, root("implicit").mir().unwrap()] {
                let (yes, no) = branch_targets(mir);
                check_cleanup(mir, yes);
                check_cleanup(mir, no);
            }
        },
    );
    // Only the installed invocation runs the callback; collect both modes before asserting.
    for verdicts in observed {
        assert_eq!(
            verdicts,
            [
                Completion::Checked,
                Completion::Checked,
                Completion::SourceRejected,
                Completion::SourceRejected
            ]
        );
    }
}

#[test]
fn list_backing_cleanup_follows_the_owner_not_the_parameter_slot() {
    probe_report_with_modules(
        "scoped_jobs::list_backing_cleanup_follows_the_owner_not_the_parameter_slot",
        r#"
        import std::core::dispose as release;
        import helper::{relay as importedRelay, consume};
        function discard(input: List<i32>): void { let _ = input; }
        function transfer(input: List<i32>): List<i32> { return input; }
        function localResult(input: List<i32>): void { let result = transfer(input); release(result); }
        function importedResult(input: List<i32>): void { let result = importedRelay(input); release(result); }
        function handoff(input: List<i32>): void { consume(input); }
        function grouped(input: List<i32>): void { (release)(((input))); }
        function escape(input: List<i32>): &List<i32> { return &input; }
        function other(input: List<bool>): void { release(input); }
    "#,
        &[(
            "helper.rill",
            "import std::{List};\nexport function relay(xs: List<i32>): List<i32> { return xs; }\nexport function consume(xs: List<i32>): void { dispose(xs); }",
        )],
        |_, module, typed| {
            let receipt = typed.receipt().expect("a typed ordinary cross-module call");
            let lower = |name: &str| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(&receipt, module, &d.id, AnalysisLimits::default()).unwrap()
            };
            for name in ["discard", "handoff"] {
                let body = lower(name);
                let mir = body.mir().unwrap();
                let blocks = straight_blocks(mir, mir.entry);
                let Operation::Move {
                    from,
                    into: Some(owned),
                } = blocks[0].operation
                else {
                    panic!("one transfer from the input")
                };
                assert_eq!(from.root.span.bytes(), b"input: List<i32>");
                assert_ne!(from.root.node, owned.root.node);
                let drops = path_drops(mir, mir.entry);
                if name == "discard" {
                    assert!(
                        matches!(drops.as_slice(),[Obligation::DisposeListI32 { operand,.. }] if operand.root.node==owned.root.node)
                    );
                } else {
                    assert!(drops.is_empty());
                    assert!(
                        matches!(blocks[1].operation, Operation::Call(id) if id.module.path=="helper.rill")
                    );
                }
            }
            let graph = analyze_ownership(&receipt, AnalysisLimits::default());
            let root = |path: &str, name: &str| {
                let prefix = format!("function {name}(");
                graph
                    .roots()
                    .iter()
                    .find(|r| {
                        r.site().module.path == path
                            && r.site().span.bytes().starts_with(prefix.as_bytes())
                    })
                    .unwrap()
            };
            let consume = root("helper.rill", "consume").mir().unwrap();
            assert!(
                matches!(path_drops(consume, consume.entry).as_slice(),[Obligation::DisposeListI32{operand,..}] if operand.root.span.bytes()==b"xs: List<i32>")
            );
            assert!(root("main.rill", "handoff").mir().is_some());
            for name in ["transfer", "localResult", "importedResult"] {
                let body = lower(name);
                assert_eq!(body.completion(), Completion::Incomplete, "{name}");
                assert!(body.mir().is_none() && body.findings().is_empty());
                assert!(root("main.rill", name).mir().is_none());
            }
            assert!(root("helper.rill", "relay").mir().is_none());
            let grouped = lower("grouped");
            let mir = grouped.mir().unwrap();
            assert!(
                matches!(path_drops(mir,mir.entry).as_slice(),[Obligation::DisposeListI32{operand,..}] if operand.root.span.bytes()==b"input: List<i32>")
            );
            let escape = lower("escape");
            assert_eq!(escape.completion(), Completion::SourceRejected);
            assert!(escape.mir().is_none());
            let [f] = escape.findings() else {
                panic!("one escape")
            };
            assert_eq!(
                (f.code(), f.site.span.bytes(), f.related.span.bytes()),
                (
                    "R0202",
                    b"return &input;".as_slice(),
                    b"input: List<i32>".as_slice()
                )
            );
            let other = lower("other");
            assert_eq!(other.completion(), Completion::Incomplete);
            assert!(other.mir().is_none() && other.findings().is_empty());
        },
    );
}

#[test]
fn installed_shared_list_literal_reads() {
    probe_with_typed(
        "scoped_jobs::installed_shared_list_literal_reads",
        r#"
            function first(input: List<i32>): i32 {
                let numbers = input;
                let view = &numbers;
                let result = view[0u64];
                dispose(numbers);
                return result;
            }
            function second(input: List<i32>): i32 {
                let numbers = input;
                let result = (&numbers)[1u64];
                dispose(numbers);
                return result;
            }
            function zero(numbers: List<i32>): i32 { return (&numbers)[0u64]; }
            function wide(input: List<i32>): i32 { return (&input)[18446744073709551615u64]; }
            function unknown(view: &List<i32>): i32 { return view[0u64]; }
            function exclusive(view: &mut List<i32>): i32 { return view[0u64]; }
            function dynamic(input: List<i32>, index: u64): i32 { return (&input)[index]; }
            function live(input: List<i32>): i32 {
                let view = &input;
                dispose(input);
                return view[0u64];
            }
            function moved(input: List<i32>): i32 {
                let numbers = input;
                dispose(numbers);
                return (&numbers)[0u64];
            }
        "#,
        |request, module, receipt, typed| {
            let limits = AnalysisLimits::default();
            let decl = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name| lower_body(receipt, module, &decl(name).id, limits).unwrap();
            let graph = analyze_ownership(receipt, limits);
            let root = |name| {
                graph
                    .roots()
                    .iter()
                    .find(|root| {
                        root.site().module == &module.key && root.site().node == decl(name).node
                    })
                    .unwrap()
            };
            assert!(
                graph
                    .open_obligations()
                    .contains(&GraphObligation::MaskingAndJoin)
            );
            let list = typed.list_role().unwrap();
            let representation = typed
                .collection_representations()
                .find(|repr| {
                    repr.storage == CollectionStorage::Elements
                        && std::ptr::eq(repr.declaration, list.declaration)
                })
                .unwrap()
                .artifact;
            let provider = typed.list_operations_artifact().unwrap();
            assert_ne!(representation, provider);
            for artifact in [representation, provider] {
                assert_eq!(*artifact.context(), receipt.context());
                assert!(artifact.executable().ends_with("bin/m5-probe"));
            }
            for (name, index, site, temporary) in [
                ("first", 0, "view[0u64]", false),
                ("second", 1, "(&numbers)[1u64]", true),
                ("zero", 0, "(&numbers)[0u64]", true),
                ("wide", u64::MAX, "(&input)[18446744073709551615u64]", true),
            ] {
                let report = lower(name);
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name} {report:?}"
                );
                let root = root(name);
                assert_eq!(root.completion(), Completion::Checked, "{name}");
                // Brackets are the direct installed operation, not a call to `get`.
                assert!(
                    root.dependencies()
                        .iter()
                        .all(|edge| edge.site.span.bytes() != site.as_bytes())
                );
                for mir in [report.mir().unwrap(), root.mir().unwrap()] {
                    let path = straight_blocks(mir, mir.entry);
                    let [read_at] = path
                        .iter()
                        .enumerate()
                        .filter_map(|(at, block)| {
                            matches!(block.operation, Operation::ReadListI32 { .. }).then_some(at)
                        })
                        .collect::<Vec<_>>()[..]
                    else {
                        panic!("exactly one checked read")
                    };
                    let Operation::ReadListI32 {
                        view,
                        backing,
                        index: read_index,
                        exclusive: false,
                        into: Some(result),
                        owner,
                        representation: repr,
                        provider: linked,
                    } = path[read_at].operation
                    else {
                        panic!("the read must produce an i32")
                    };
                    assert_eq!(path[read_at].site.span.bytes(), site.as_bytes());
                    assert_eq!(read_index, index);
                    assert!(std::ptr::eq(owner, &list.declaration.id));
                    assert_eq!((repr, linked), (representation, provider));
                    assert!(matches!(path[read_at - 1].operation,
                        Operation::Borrow { source, holder, exclusive: false }
                            if source.root.node == backing.root.node && holder.root.node == view.root.node));
                    if temporary {
                        assert!(
                            matches!(&path[read_at + 1].operation, Operation::Cleanup(items)
                            if matches!(items.as_slice(), [Obligation::EndStorage(ended)]
                                if ended.root.node == view.root.node))
                        );
                    }
                    let [Obligation::DisposeListI32 { operand, .. }] =
                        path_drops(mir, mir.entry)[..]
                    else {
                        panic!("exactly one backing cleanup")
                    };
                    assert_eq!(operand.root.node, backing.root.node);
                    assert!(path[read_at + 1..].iter().any(|block| matches!(&block.operation,
                        Operation::Cleanup(items) if items.iter().any(|item|
                            matches!(item, Obligation::DisposeListI32 { operand, .. } if operand.root.node == backing.root.node)))));
                    let exported = path
                        .iter()
                        .find_map(|block| match block.operation {
                            Operation::Export(place) => Some(place),
                            _ => None,
                        })
                        .unwrap();
                    assert!(exported.root.node == result.root.node || path[read_at + 1..].iter().any(|block|
                        matches!(block.operation, Operation::Copy { from, into: Some(into) }
                            if from.root.node == result.root.node && into.root.node == exported.root.node)));
                    assert!(mir.cancellation().is_none());
                }
                if name == "second" {
                    check_exact(receipt, module, &decl(name).id, report.statistics());
                }
            }
            for (name, site) in [
                ("unknown", "view[0u64]"),
                ("exclusive", "view[0u64]"),
                ("dynamic", "(&input)[index]"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site.as_bytes());
                assert!(report.findings().is_empty() && report.mir().is_none());
                assert_eq!(root(name).completion(), Completion::Incomplete);
                assert!(root(name).mir().is_none());
            }
            for (name, code, site, related) in [
                ("live", "R0203", "dispose(input)", "&input"),
                ("moved", "R0200", "&numbers", "dispose(numbers)"),
            ] {
                let report = lower(name);
                assert_eq!(
                    report.completion(),
                    Completion::SourceRejected,
                    "{name} {report:?}"
                );
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("one ownership finding")
                };
                assert_eq!(
                    (
                        finding.code(),
                        finding.site.span.bytes(),
                        finding.related.span.bytes()
                    ),
                    (code, site.as_bytes(), related.as_bytes())
                );
                assert_eq!(root(name).completion(), Completion::SourceRejected);
                assert!(root(name).mir().is_none());
            }

            let installed = loader::load_installed(request.clone()).unwrap();
            let foreign = installed
                .modules()
                .iter()
                .find(|m| m.key == module.key)
                .unwrap();
            let foreign_id = &foreign
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("first"))
                .unwrap()
                .id;
            for (module, id) in [(foreign, &decl("first").id), (module, foreign_id)] {
                assert_eq!(
                    lower_body(receipt, module, id, limits).unwrap_err(),
                    QueryError::InvalidRequest
                );
            }
            let source_only = loader::load(installed.request().clone()).unwrap();
            let wrong_target = loader::load_installed(InstalledRequest {
                target: "wasm32-wasip1".into(),
                ..request.clone()
            })
            .unwrap();
            for graph in [&source_only, &wrong_target] {
                let typed = analyze_typed(graph, limits);
                assert!(typed.receipt().is_none());
                assert!(typed.list_operations_artifact().is_none());
                assert!(typed.collection_representations().next().is_none());
            }
        },
    );
}

#[test]
fn grouped_list_index_and_unadmitted_selectors() {
    probe(
        "scoped_jobs::grouped_list_index_and_unadmitted_selectors",
        r#"
            function bare(xs: List<i32>): i32 { return (&xs)[1u64]; }
            function grouped(xs: List<i32>): i32 { return ((&((xs))))[((((((1u64))))))]; }
            function named(xs: List<i32>): i32 { let view = &xs; let alias = view; return ((alias))[(0u64)]; }
            function dynamic(xs: List<i32>, i: u64): i32 { return (&xs)[i]; }
            function unknown(view: &List<i32>): i32 { return view[0u64]; }
            function exclusive(view: &mut List<i32>): i32 { return view[0u64]; }
        "#,
        |_, module, receipt| {
            let run = |name: &str, limits| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            for (name, index) in [("bare", 1), ("grouped", 1), ("named", 0)] {
                let report = run(name, default);
                let mir = report.mir().expect("shared local index read");
                let mut reads = mir.blocks.iter().filter_map(|b| match b.operation {
                    Operation::ReadListI32 { index, backing, .. } => {
                        Some((index, backing.root.span.bytes()))
                    }
                    _ => None,
                });
                assert_eq!(reads.next(), Some((index, b"xs: List<i32>".as_slice())));
                assert!(reads.next().is_none());
            }
            let depth = run("grouped", default).statistics().max_depth;
            assert!(depth > run("bare", default).statistics().max_depth);
            let limited = run(
                "grouped",
                AnalysisLimits {
                    max_depth: depth - 1,
                    ..default
                },
            );
            crate::limit::assert_body_limit(&limited);
            // Exact budgets for these unchanged pending routes on 9b020d1.
            for (name, max_work, max_entries) in [
                ("dynamic", 2020, 88),
                ("unknown", 1994, 84),
                ("exclusive", 1994, 84),
            ] {
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
        },
    );
}

#[test]
fn overflowing_list_selector_is_static_invalid() {
    probe_report(
        "scoped_jobs::overflowing_list_selector_is_static_invalid",
        "function invalid(numbers: List<i32>): i32 { return (&numbers)[18446744073709551616u64]; }",
        |_, module, typed| {
            // A static literal diagnostic can coexist with incomplete aggregate facts.
            assert!(typed.receipt().is_none());
            assert!(
                typed
                    .bodies()
                    .findings()
                    .iter()
                    .any(|finding| finding.kind == ScalarFindingKind::LiteralRange
                        && finding.site.module == &module.key
                        && finding.site.span.bytes() == b"18446744073709551616u64")
            );
        },
    );
}

#[test]
fn list_arrow_invocations_still_need_typed_environment_evidence() {
    probe_report(
        "scoped_jobs::list_arrow_invocations_still_need_typed_environment_evidence",
        r#"
            function known(input: List<i32>): i32 {
                let numbers = input;
                let first = () => (&numbers)[0u64];
                let second = () => (&numbers)[1u64];
                return first() + second();
            }
            function erased(job: () => i32): i32 { return job(); }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.core_blockers().is_empty());
            assert!(typed.receipt().is_none());
            let pending = typed.result_use().unwrap().pending();
            for (site, blocker) in [
                ("first()", ResultUseBlocker::CallBoundary),
                ("second()", ResultUseBlocker::CallBoundary),
                ("job()", ResultUseBlocker::CallBoundary),
            ] {
                assert!(
                    pending.iter().any(|use_| use_.blocker == blocker
                        && use_.site.module == &module.key
                        && use_.site.span.bytes() == site.as_bytes()),
                    "{pending:?}"
                );
            }
        },
    );
}

fn check_exact(
    receipt: &TypedReceipt<'_, '_>,
    module: &loader::Module,
    id: &loader::DeclarationId,
    stats: Statistics,
) {
    eprintln!("ownership limits: {stats:?}");
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    assert!(
        lower_body(receipt, module, id, exact)
            .unwrap()
            .mir()
            .is_some()
    );
    for short in [
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
    ] {
        let lower = lower_body(receipt, module, id, short).unwrap();
        crate::limit::assert_body_limit(&lower);
    }
}
