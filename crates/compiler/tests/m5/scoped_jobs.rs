use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{DependencyContract, GraphObligation, analyze_ownership};
use compiler::analysis::typed::{CollectionOperation, CollectionStorage, OwnershipCoverage};

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
        assert_eq!(lower.completion(), Completion::ResourceLimit);
        assert!(lower.mir().is_none());
    }
}
