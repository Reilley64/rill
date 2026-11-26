use super::*;

#[path = "patterns/commitment.rs"]
mod commitment;

#[test]
fn owned_narrow_variant_exports_the_lexical_payload() {
    use compiler::analysis::{
        ownership::{GraphCompletion, Place, Projection, analyze_ownership},
        scalars::BodyOwner,
    };
    use loader::DeclarationKind;

    let mut normal_context = None;
    probe(
        "patterns::owned_narrow_variant_exports_the_lexical_payload",
        r#"
        enum Parcel { Text(string), Empty }
        function keep(input: Parcel.Text): string { let Parcel.Text(text) = input; return text; }
        function annotated(input: Parcel.Text): string { let Parcel.Text(text): Parcel.Text = input; return text; }
        function discard(input: Parcel.Text, after: string): string { let Parcel.Text(_) = input; return after; }
        function reuse(input: Parcel.Text): string { let Parcel.Text(first) = input; let Parcel.Text(second) = input; return second; }
        function untouched(input: Parcel.Text): void {}
        function bound(input: Parcel.Text, after: bool): bool { let Parcel.Text(text) = input; return after; }
        enum ScalarParcel { Text(i32), Owned(string), Empty }
        function copyTwice(input: ScalarParcel.Text): i32 { let ScalarParcel.Text(first) = input; let ScalarParcel.Text(second) = input; return second; }
        function groupedSource(input: Parcel.Text): string { let Parcel.Text(text) = (((((((input))))))); return text; }
        function groupedPattern(input: Parcel.Text): string { let (((Parcel.Text((((text))))))) = input; return text; }
    "#,
        |request, module, receipt| {
            let default = AnalysisLimits::default();
            let function = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name, limits| lower_body(receipt, module, &function(name).id, limits).unwrap();
            let [text, scalar] = module
                .declarations
                .iter()
                .filter(|d| {
                    d.id.kind == DeclarationKind::Variant && d.name.as_deref() == Some("Text")
                })
                .collect::<Vec<_>>()[..]
            else {
                panic!("expected two distinct Text variant declarations")
            };
            assert_ne!(text.id, scalar.id);
            let payload = |place: &Place<'_>, variant| matches!(place.projection, Some(Projection::VariantPayload { variant: id, index: 0 }) if id == variant);
            let verify = |name, mir: &Mir<'_>| {
                let ops = linear(mir, mir.entry);
                assert!(
                    !ops.iter()
                        .any(|op| matches!(op, Operation::TestVariant { .. }))
                );
                let transfers: Vec<_> = ops
                    .iter()
                    .copied()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::Copy { .. } | Operation::Move { .. } | Operation::Export(_)
                        )
                    })
                    .collect();
                let drops = path_drops(mir, mir.entry);
                match name {
                    "keep" | "annotated" | "groupedSource" | "groupedPattern" => {
                        assert!(matches!(transfers.as_slice(), [
                            Operation::Move { from: input, into: Some(stage) },
                            Operation::Move { from: leaf, into: Some(local) },
                            Operation::Move { from: giving, into: Some(result) },
                            Operation::Export(export),
                        ] if input.root.span.bytes() == b"input: Parcel.Text" && input.projection.is_none()
                            && stage.projection.is_none() && stage.root.node != input.root.node
                            && leaf.root.node == stage.root.node && payload(leaf, &text.id)
                            && local.root.span.bytes() == b"text" && local.projection.is_none()
                            && local.root.node == giving.root.node && giving.projection.is_none()
                            && result.root.node == export.root.node && export.projection.is_none()));
                        assert!(drops.is_empty());
                    }
                    "discard" => {
                        let [
                            Operation::Move {
                                from: input,
                                into: Some(stage),
                            },
                            Operation::Move {
                                from: after,
                                into: Some(result),
                            },
                            Operation::Export(export),
                        ] = transfers.as_slice()
                        else {
                            panic!("{transfers:?}")
                        };
                        assert_eq!(input.root.span.bytes(), b"input: Parcel.Text");
                        assert_eq!(after.root.span.bytes(), b"after: string");
                        assert_eq!(result.root.node, export.root.node);
                        assert!(
                            matches!(drops.as_slice(), [Obligation::Drop(leaf)] if leaf.root.node == stage.root.node && payload(leaf, &text.id))
                        );
                        let drop = ops.iter().position(|op| matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action, Obligation::Drop(leaf) if payload(leaf, &text.id))))).unwrap();
                        let stage_move = ops
                            .iter()
                            .position(|op| std::ptr::eq(*op, transfers[0]))
                            .unwrap();
                        let suffix = ops
                            .iter()
                            .position(|op| std::ptr::eq(*op, transfers[1]))
                            .unwrap();
                        assert!(stage_move < drop && drop < suffix);
                    }
                    "untouched" => {
                        assert!(transfers.is_empty());
                        assert!(
                            matches!(drops.as_slice(), [Obligation::Drop(leaf)] if leaf.root.span.bytes() == b"input: Parcel.Text" && payload(leaf, &text.id))
                        );
                    }
                    "bound" => {
                        let [
                            Operation::Move {
                                into: Some(stage), ..
                            },
                            Operation::Move {
                                from: leaf,
                                into: Some(local),
                            },
                            Operation::Copy { from: after, .. },
                            Operation::Export(_),
                        ] = transfers.as_slice()
                        else {
                            panic!("{transfers:?}")
                        };
                        assert!(leaf.root.node == stage.root.node && payload(leaf, &text.id));
                        assert_eq!(local.root.span.bytes(), b"text");
                        assert_eq!(after.root.span.bytes(), b"after: bool");
                        assert!(
                            matches!(drops.as_slice(), [Obligation::Drop(drop)] if drop.root.node == local.root.node && drop.projection.is_none())
                        );
                        let suffix = ops
                            .iter()
                            .position(|op| std::ptr::eq(*op, transfers[2]))
                            .unwrap();
                        let drop = ops.iter().position(|op| matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action, Obligation::Drop(place) if place.root.node == local.root.node)))).unwrap();
                        assert!(suffix < drop);
                    }
                    "copyTwice" => {
                        assert!(matches!(transfers.as_slice(), [
                            Operation::Copy { from: input, into: Some(stage1) },
                            Operation::Copy { from: leaf1, into: Some(first) },
                            Operation::Copy { from: reuse, into: Some(stage2) },
                            Operation::Copy { from: leaf2, into: Some(second) },
                            Operation::Copy { from: giving, into: Some(result) },
                            Operation::Export(export),
                        ] if input.root.span.bytes() == b"input: ScalarParcel.Text" && input.projection.is_none()
                            && input.root.node == reuse.root.node && reuse.projection.is_none()
                            && stage1.root.node != stage2.root.node && stage1.projection.is_none() && stage2.projection.is_none()
                            && leaf1.root.node == stage1.root.node && payload(leaf1, &scalar.id)
                            && leaf2.root.node == stage2.root.node && payload(leaf2, &scalar.id)
                            && first.root.span.bytes() == b"first" && second.root.span.bytes() == b"second"
                            && second.root.node == giving.root.node && result.root.node == export.root.node));
                        assert!(drops.is_empty());
                    }
                    _ => unreachable!(),
                }
            };
            let rejected = |findings: &[compiler::analysis::ownership::Finding<'_>]| {
                let [finding] = findings else {
                    panic!("expected exactly one R0200")
                };
                let source = function("reuse").span.bytes();
                let read = |needle: &[u8]| {
                    function("reuse").span.start()
                        + source
                            .windows(needle.len())
                            .position(|bytes| bytes == needle)
                            .unwrap()
                        + needle.len()
                        - b"input;".len()
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), b"input");
                assert_eq!(finding.site.span.start(), read(b"(second) = input;"));
                assert_eq!(finding.related.span.bytes(), b"input");
                assert_eq!(finding.related.span.start(), read(b"(first) = input;"));
            };
            let graph = analyze_ownership(receipt, default);
            assert_eq!(graph.context(), receipt.context());
            assert_eq!(graph.test_mode(), request.test_mode);
            if let Some(normal) = &normal_context {
                assert_ne!(normal, &graph.context());
            } else {
                normal_context = Some(graph.context());
            }
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            assert!(graph.inventory_complete());
            rejected(graph.findings());
            let root = |name| {
                graph
                    .roots()
                    .iter()
                    .find(|root| root.owner() == BodyOwner::Declaration(&function(name).id))
                    .unwrap()
            };
            let report = lower("reuse", default);
            assert_eq!(report.completion(), Completion::SourceRejected);
            rejected(report.findings());
            assert!(report.mir().is_none());
            assert_eq!(
                root("reuse").ordinary_completion(),
                Completion::SourceRejected
            );
            assert!(root("reuse").ordinary_mir().is_none());
            for name in [
                "keep",
                "annotated",
                "discard",
                "untouched",
                "bound",
                "copyTwice",
                "groupedSource",
                "groupedPattern",
            ] {
                let local = lower(name, default);
                assert_eq!(
                    local.completion(),
                    Completion::Checked,
                    "{name}: {:?}",
                    local
                        .blocked_at()
                        .map(|site| site.span.bytes().escape_ascii().to_string())
                );
                assert_eq!(
                    root(name).ordinary_completion(),
                    Completion::Checked,
                    "{name}"
                );
                verify(name, local.mir().unwrap());
                verify(name, root(name).ordinary_mir().unwrap());
            }
            let bare_depth = lower("keep", default).statistics().max_depth;
            for name in ["groupedSource", "groupedPattern"] {
                let stats = lower(name, default).statistics();
                assert!(stats.max_depth > bare_depth);
                let exact = AnalysisLimits {
                    max_depth: stats.max_depth,
                    max_work: stats.work,
                    max_entries: stats.entries,
                };
                assert_eq!(lower(name, exact).completion(), Completion::Checked);
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
                    let limited = lower(name, short);
                    crate::limit::assert_body_limit(&limited);
                }
            }
            let stats = graph.statistics();
            let exact = AnalysisLimits {
                max_depth: stats.max_depth,
                max_work: stats.work,
                max_entries: stats.entries,
            };
            assert_eq!(
                analyze_ownership(receipt, exact).completion(),
                GraphCompletion::SourceRejected
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
            ] {
                crate::limit::assert_graph_limit(&analyze_ownership(receipt, short));
            }
            let shallow = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..default
                },
            );
            crate::limit::assert_graph_limit(&shallow);
            for name in ["groupedSource", "groupedPattern"] {
                let root = shallow
                    .roots()
                    .iter()
                    .find(|root| root.owner() == BodyOwner::Declaration(&function(name).id))
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::ResourceLimit);
                assert!(root.ordinary_mir().is_none());
            }
        },
    );
}

#[test]
fn narrow_variant_early_exit_keeps_only_the_reached_owner() {
    use compiler::analysis::ownership::Projection;
    installed(
        "patterns::narrow_variant_early_exit_keeps_only_the_reached_owner",
        |request| {
            let mut short = [AnalysisLimits::default(); 2];
            for width in [1, 64] {
                let name = "v".repeat(width);
                fs::write(
                    &request.entry,
                    roots::MAIN.replace("import helper::ready;", &format!("import {name}::ready;")),
                )
                .unwrap();
                let file = format!("{name}.rill");
                fs::write(
                    request.entry.with_file_name(&file),
                    r#"
                export function ready(): bool { return true; }
                enum Parcel { Empty, Text(string) }
                function branch(input: Parcel.Text, early: bool): bool {
                    if (early) { return true; }
                    let Parcel.Text(text) = input;
                    return false;
                }
            "#,
                )
                .unwrap();
                for (i, test_mode) in [false, true].into_iter().enumerate() {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    let receipt = typed.receipt().unwrap();
                    let module = graph.modules().iter().find(|m| m.key.path == file).unwrap();
                    let decl = module
                        .declarations
                        .iter()
                        .find(|d| d.name.as_deref() == Some("branch"))
                        .unwrap();
                    let variant = module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.name.as_deref() == Some("Text")
                                && d.id.kind == loader::DeclarationKind::Variant
                        })
                        .unwrap();
                    let run = |limits| lower_body(&receipt, module, &decl.id, limits).unwrap();
                    let result = run(AnalysisLimits::default());
                    let mir = result
                        .mir()
                        .expect("both exits have exactly one initialized owner");
                    let (yes, no) = branch_targets(mir);
                    assert!(
                        matches!(path_drops(mir,yes).as_slice(), [Obligation::Drop(p)] if p.root.span.bytes()==b"input: Parcel.Text" && matches!(p.projection, Some(Projection::VariantPayload{variant:id,index:0}) if std::ptr::eq(id,&variant.id)))
                    );
                    assert!(
                        matches!(path_drops(mir,no).as_slice(), [Obligation::Drop(p)] if p.root.span.bytes()==b"text" && p.projection.is_none())
                    );
                    let stats = result.statistics();
                    if width == 1 {
                        short[i] = AnalysisLimits {
                            max_work: stats.work,
                            max_entries: stats.entries,
                            max_depth: stats.max_depth,
                        };
                    } else {
                        assert_eq!(
                            (stats.entries, stats.max_depth),
                            (short[i].max_entries, short[i].max_depth)
                        );
                        let exhausted = run(short[i]);
                        crate::limit::assert_body_limit(&exhausted);
                    }
                }
            }
        },
    );
}

#[test]
fn owned_narrow_variant_keeps_other_storage_routes_incomplete() {
    use compiler::analysis::{ownership::analyze_ownership, scalars::BodyOwner};

    probe(
        "patterns::owned_narrow_variant_keeps_other_storage_routes_incomplete",
        r#"
        enum Parcel { Text(string), Guard(LockGuard<i32>), Nested([string, bool]), Empty }
        enum Singleton { Text(string) }
        enum Generic<T> { Text(T), Empty }
        function borrowed(input: &Parcel.Text): &string { let Parcel.Text(text) = input; return text; }
        function groupedInput(input: &Parcel.Text): &string { let Parcel.Text(text) = ((((((((((input)))))))))); return text; }
        function groupedChild(input: &Parcel.Text): &string { let Parcel.Text(((((((((((text))))))))))) = input; return text; }
        function protected(input: Parcel.Guard): void { let Parcel.Guard(_) = input; }
        function nested(input: Parcel.Nested): void { let Parcel.Nested([_, _]) = input; }
        function parent(input: Singleton): string { let Singleton.Text(text) = input; return text; }
        function generic(input: Generic<string>.Text): void { let Generic<string>.Text(_) = input; }
        function whole(input: Parcel.Text): void { let saved = input; }
    "#,
        |_, module, receipt| {
            let limits = AnalysisLimits::default();
            let graph = analyze_ownership(receipt, limits);
            for name in [
                "borrowed",
                "groupedInput",
                "groupedChild",
                "protected",
                "nested",
                "parent",
                "generic",
                "whole",
            ] {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report = lower_body(receipt, module, &decl.id, limits).unwrap();
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none() && report.findings().is_empty());
                let root = graph
                    .roots()
                    .iter()
                    .find(|root| root.owner() == BodyOwner::Declaration(&decl.id))
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Incomplete, "{name}");
                assert!(root.ordinary_mir().is_none());
            }
            assert!(graph.findings().is_empty());
            let run = |name: &str, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let tight = AnalysisLimits {
                max_depth: run("borrowed", limits).statistics().max_depth,
                ..limits
            };
            assert_eq!(run("borrowed", tight).completion(), Completion::Incomplete);
            for name in ["groupedInput", "groupedChild"] {
                let short = run(name, tight);
                crate::limit::assert_body_limit(&short);
            }
        },
    );
}

#[test]
fn owned_narrow_variant_refutable_bindings_have_no_typed_receipt() {
    use compiler::analysis::{scalars::ScalarFindingKind, typed::TypedCompletion};

    probe_report(
        "patterns::owned_narrow_variant_refutable_bindings_have_no_typed_receipt",
        r#"
        enum Parcel { Text(string), Empty }
        function parent(input: Parcel): string { let Parcel.Text(text) = input; return text; }
        function refutablePayload(input: Parcel.Text): void { let Parcel.Text("test") = input; }
    "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let refutable: Vec<_> = typed
                .bodies()
                .findings()
                .iter()
                .filter(|f| f.kind == ScalarFindingKind::RefutableBinding)
                .map(|f| f.site.span.bytes())
                .collect();
            assert_eq!(
                refutable,
                [
                    b"Parcel.Text(text)".as_slice(),
                    b"Parcel.Text(\"test\")".as_slice()
                ]
            );
        },
    );
}

#[test]
fn owned_narrow_variant_custom_disposer_has_no_typed_receipt() {
    use compiler::analysis::typed::TypedCompletion;

    probe_report(
        "patterns::owned_narrow_variant_custom_disposer_has_no_typed_receipt",
        r#"
        enum Parcel { Text(string), Empty }
        implement dispose(value: Parcel.Text): void {}
        function untouched(input: Parcel.Text): void {}
    "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
        },
    );
}

#[test]
fn constructed_nested_record_pattern_cleans_named_binding_on_option_exits() {
    use compiler::analysis::ownership::{Place, Projection};

    probe_with_typed(
        "patterns::constructed_nested_record_pattern_cleans_named_binding_on_option_exits",
        r#"
            type Inner = { text: string; };
            type Outer = { first: Inner; second: Inner; };
            function exits(early: Inner, late: Inner, input: Option<bool>): Option<bool> {
                let owner = Outer { second: late, first: early };
                let Outer { first: Inner { text: selected } } = owner;
                let bit = input?;
                let tail = selected;
                return Option.Some(bit);
            }
            function movedBeforePattern(early: Inner, late: Inner): void {
                let owner = Outer { second: late, first: early };
                let moved = owner;
                let Outer { first: Inner { text: selected } } = owner;
            }
            function movedByPattern(early: Inner, late: Inner): void {
                let owner = Outer { second: late, first: early };
                let Outer { first: Inner { text: selected } } = owner;
                let reused = owner;
            }
        "#,
        |_, module, receipt, typed| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let field = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == loader::DeclarationKind::Field
                            && d.name.as_deref() == Some(name)
                    })
                    .unwrap()
                    .id
            };
            let role = typed.option_role().expect("admitted Option role");
            let report = lower("exits");
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "exits: {:?} at {:?}",
                    report.completion(),
                    report.blocked_at().map(|s| s.span.bytes())
                )
            });
            let whole = |place: &Place<'_>, root: &Place<'_>| {
                place.root.node == root.root.node
                    && place.projection.is_none()
                    && root.projection.is_none()
            };
            let member = |place: &Place<'_>, root: &Place<'_>, name| {
                place.root.node == root.root.node
                    && place.field().is_some_and(|d| &d.id == field(name))
            };
            let leaf = |place: &Place<'_>, root: &Place<'_>, child| {
                place.root.node == root.root.node
                    && matches!(place.projection, Some(Projection::NestedField { outer, inner })
                        if &outer.id == field(child) && &inner.id == field("text"))
            };
            let prefix = straight_blocks(mir, mir.entry);
            let (test, setup) = prefix.split_last().unwrap();
            let (Operation::TestVariant { operand, success }, Terminator::Branch { yes, no }) =
                (&test.operation, test.terminator)
            else {
                panic!("postfix Option must test a variant with two feasible exits");
            };
            let ops: Vec<_> = setup
                .iter()
                .map(|block| &block.operation)
                .filter(|op| !matches!(op, Operation::None))
                .collect();
            let [
                Operation::Move {
                    from: late,
                    into: Some(second),
                },
                Operation::Move {
                    from: early,
                    into: Some(first),
                },
                Operation::Initialize(owner),
                Operation::Move {
                    from: source,
                    into: Some(scratch),
                },
                Operation::Move {
                    from: text,
                    into: Some(selected),
                },
                Operation::Cleanup(remainder),
                Operation::Copy {
                    from: input,
                    into: Some(staged),
                },
            ] = ops.as_slice()
            else {
                panic!(
                    "expected construction, one pattern stage, temporary cleanup, then propagation"
                );
            };
            for (place, span) in [
                (late, b"late: Inner".as_slice()),
                (early, b"early: Inner"),
                (owner, b"owner"),
                (scratch, b"owner"),
                (selected, b"selected"),
                (input, b"input: Option<bool>"),
            ] {
                assert_eq!(place.root.span.bytes(), span);
                assert!(place.projection.is_none());
            }
            assert_ne!(owner.root.node, scratch.root.node);
            assert!(member(second, owner, "second") && member(first, owner, "first"));
            assert!(whole(source, owner) && leaf(text, scratch, "first"));
            assert!(
                matches!(remainder.as_slice(), [Obligation::Drop(unbound), Obligation::EndStorage(second), Obligation::EndStorage(first), Obligation::EndStorage(root)]
                if leaf(unbound, scratch, "second") && member(second, scratch, "second")
                    && member(first, scratch, "first") && whole(root, scratch))
            );
            assert!(whole(operand, staged));
            assert_eq!(*success, &role.some.id);

            let path = |entry| {
                linear(mir, entry)
                    .into_iter()
                    .filter(|op| !matches!(op, Operation::None))
                    .collect::<Vec<_>>()
            };
            assert!(
                matches!(path(yes).as_slice(), [
                Operation::CopyOptionPayload { operand, into: bit },
                Operation::Move { from, into: Some(tail) },
                Operation::Copy { from: result, into: Some(payload) },
                Operation::ConstructVariant { variant, payload: Some(value), into: output },
                Operation::Export(export),
                Operation::Cleanup(locals),
                Operation::Cleanup(parameters),
            ] if whole(operand, staged) && whole(from, selected)
                && tail.root.span.bytes() == b"tail" && tail.projection.is_none()
                && bit.root.span.bytes() == b"bit" && whole(result, bit) && whole(value, payload)
                && *variant == &role.some.id && whole(export, output)
                && matches!(locals.as_slice(), [Obligation::Drop(bound), Obligation::EndStorage(flag)]
                    if whole(bound, tail) && whole(flag, bit))
                && matches!(parameters.as_slice(), [Obligation::EndStorage(param)] if whole(param, input))),
                "Some must export the bound payload before dropping tail"
            );
            assert!(
                matches!(path(no).as_slice(), [
                Operation::ConstructVariant { variant, payload: None, into: output },
                Operation::Export(export),
                Operation::Cleanup(locals),
                Operation::Cleanup(parameters),
            ] if *variant == &role.none.id && whole(export, output)
                && matches!(locals.as_slice(), [Obligation::Drop(bound)] if whole(bound, selected))
                && matches!(parameters.as_slice(), [Obligation::EndStorage(param)] if whole(param, input))),
                "None must export before dropping the still-initialized selected binding"
            );

            for name in ["movedBeforePattern", "movedByPattern"] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none(), "{name}");
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected one moved-owner finding");
                };
                assert_eq!(finding.code(), "R0200", "{name}");
                assert_eq!(finding.site.span.bytes(), b"owner", "{name}");
                assert_eq!(finding.related.span.bytes(), b"owner", "{name}");
                assert!(
                    finding.related.span.start() < finding.site.span.start(),
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn owned_record_binding_transfers_fields_and_cleans_the_remainder() {
    probe(
        "patterns::owned_record_binding_transfers_fields_and_cleans_the_remainder",
        r#"
            type Packet = { left: string; skip: string; right: string; omit: string; };
            function pass(value: Packet): Packet { return value; }
            function unpack(value: Packet): string {
                let (Packet { right: (last), skip: _, left }) = pass(value);
                return last;
            }
            function reuse(value: Packet): string {
                let Packet { left: first } = value;
                return value.left;
            }
            function alreadyPartial(value: Packet): string {
                let old = value.omit;
                let Packet { left: first } = value;
                return first;
            }
            type Ref = { carried: &bool; tag: string; };
            function pointsOutside(value: Ref): &bool {
                let Ref { carried } = value;
                return carried;
            }
            function pointsInside(flag: bool): &bool {
                let Ref { carried: saved } = Ref { carried: &flag, tag: "tag" };
                return saved;
            }
            function liveBacking(seed: &bool): bool {
                let mut out = seed;
                {
                    let flag = true;
                    let Ref { carried: saved } = Ref { carried: &flag, tag: "tag" };
                    out = saved;
                }
                return *out;
            }
            type Flags = { selected: bool; answer: bool; };
            function copyable(value: Flags): bool {
                let Flags { selected: bit } = value;
                return value.answer;
            }
            type Nested = { coordinates: [bool, bool]; };
            function nested(value: Nested): bool {
                let Nested { coordinates: [x, _] } = value;
                return x;
            }
            type Custom = { text: string; };
            implement dispose(value: Custom): void {}
            function custom(value: Custom): string {
                let Custom { text } = value;
                return text;
            }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let field = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == loader::DeclarationKind::Field
                            && d.name.as_deref() == Some(name)
                    })
                    .unwrap()
            };
            let report = lower("unpack");
            let mir = report.mir().expect("flat record binding is checked");
            let mut events = Vec::new();
            let mut scratch = None;
            let mut result = None;
            let mut bound = None;
            let mut returned = false;
            let mut pc = mir.entry;
            for _ in 0..mir.blocks.len() {
                let block = &mir.blocks[pc];
                match &block.operation {
                    Operation::Call(callee) => {
                        assert_eq!(*callee, &declaration("pass").id);
                        scratch = Some(block.site.node);
                        events.push(("initialize", None));
                    }
                    Operation::Move {
                        from,
                        into: Some(to),
                    } if let Some(member) = from.field() => {
                        assert_eq!(Some(from.root.node), scratch);
                        let name = std::str::from_utf8(to.root.span.bytes()).unwrap();
                        if name == "last" {
                            bound = Some(to.root.node);
                        }
                        events.push((name, Some(&member.id)));
                    }
                    Operation::Move {
                        from,
                        into: Some(to),
                    } if Some(from.root.node) == bound && bound.is_some() => {
                        result = Some(to.root.node);
                        events.push(("stage return", None));
                    }
                    Operation::Export(output) => {
                        assert_eq!(Some(output.root.node), result);
                        events.push(("export", None));
                    }
                    Operation::Cleanup(obligations) => {
                        for obligation in obligations {
                            if let Obligation::Drop(place) = obligation {
                                if let Some(member) = place.field() {
                                    assert_eq!(Some(place.root.node), scratch);
                                    events.push(("remaining", Some(&member.id)));
                                } else {
                                    assert_eq!(place.root.span.bytes(), b"left");
                                    events.push(("local", None));
                                }
                            }
                        }
                    }
                    _ => {}
                }
                match block.terminator {
                    Terminator::Goto(next) => pc = next,
                    Terminator::Return => {
                        returned = true;
                        break;
                    }
                    _ => panic!("expected a straight binding path"),
                }
            }
            assert!(returned, "the path must reach Return");
            assert_eq!(
                events,
                [
                    ("initialize", None),
                    ("last", Some(&field("right").id)),
                    ("left", Some(&field("left").id)),
                    ("remaining", Some(&field("omit").id)),
                    ("remaining", Some(&field("skip").id)),
                    ("stage return", None),
                    ("export", None),
                    ("local", None),
                ]
            );
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("unpack", limits).completion(), Completion::Checked);
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = check("unpack", short);
                crate::limit::assert_body_limit(&limited);
            }

            let carried = lower("pointsOutside");
            let mir = carried
                .mir()
                .expect("copying a carried field does not borrow the pattern temporary");
            let ops = linear(mir, mir.entry);
            let route: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| matches!(op, Operation::Copy { .. } | Operation::Export(_)))
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: member, into: Some(bound) },
                Operation::Copy { from: read, into: Some(output) },
                Operation::Export(exported),
            ] if member.field().is_some_and(|member| member.id == field("carried").id)
                && bound.root.span.bytes() == b"carried" && bound.root.node == read.root.node
                && output.root.node == exported.root.node));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.field().is_some_and(|member| member.id == field("tag").id))
            );

            let copyable = lower("copyable");
            let mir = copyable
                .mir()
                .expect("an owned copyable record remains readable");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::Move { .. } | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: original, into: Some(scratch) },
                Operation::Copy { from: selected, into: Some(bit) },
                Operation::Copy { from: answer, into: Some(output) },
                Operation::Export(exported),
            ] if original.root.span.bytes() == b"value: Flags" && selected.root.node == scratch.root.node
                && selected.field().is_some_and(|member| member.id == field("selected").id)
                && bit.root.span.bytes() == b"bit" && answer.root.node == original.root.node
                && answer.field().is_some_and(|member| member.id == field("answer").id)
                && output.root.node == exported.root.node));

            let live = lower("liveBacking");
            assert_eq!(live.completion(), Completion::SourceRejected);
            assert!(live.mir().is_none());
            let [finding] = live.findings() else {
                panic!("expected live carried backing conflict")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.related.span.bytes(), b"&flag");
            assert!(
                finding
                    .site
                    .span
                    .bytes()
                    .starts_with(b"{\n                    let flag")
            );
            for (name, code, at, cause) in [
                ("reuse", "R0200", "value.left", "value"),
                ("alreadyPartial", "R0200", "value", "value.omit"),
                ("pointsInside", "R0202", "return saved;", "flag: bool"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none(), "{name}");
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(
                    std::str::from_utf8(finding.site.span.bytes()),
                    Ok(at),
                    "{name} primary"
                );
                assert_eq!(
                    std::str::from_utf8(finding.related.span.bytes()),
                    Ok(cause),
                    "{name} related"
                );
            }
            for name in ["custom", "nested"] {
                let incomplete = lower(name);
                assert_eq!(incomplete.completion(), Completion::Incomplete, "{name}");
                assert!(incomplete.mir().is_none(), "{name}");
            }
        },
    );
}

#[test]
fn shared_record_binding_uses_canonical_fields_and_retains_backing() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, SharedView},
        Projection,
    };

    probe(
        "patterns::shared_record_binding_uses_canonical_fields_and_retains_backing",
        r#"
            type Spare = { text: bool; flag: string; };
            type Packet = { flag: bool; text: string; skip: string; omit: string; };
            function forward(value: &Packet): &string {
                let (Packet { text: (label), skip: _, flag }) = value;
                let _ = flag;
                return label;
            }
            function ignores(value: &Packet): void { let Packet { skip: _, text: _ } = value; }
            function copied(value: Packet): bool {
                let Packet { flag } = &value;
                let moved = value;
                return flag;
            }
            function escapes(value: Packet): &string {
                let Packet { text: label } = &value;
                return label;
            }
            function premature(value: Packet): void {
                let mut owner = value;
                let whole = &owner;
                let Packet { text: label } = whole;
                owner.skip = "new";
                let _ = label;
            }
            function lastUse(value: Packet): void {
                let mut owner = value;
                let Packet { text: label } = &owner;
                let _ = label;
                owner.skip = "new";
            }
            function expires(seed: &string, flag: bool, value: Packet): &string {
                let mut result = seed;
                if (flag) { let local = value; let Packet { text: label } = &local; result = label; }
                return result;
            }
            type Custom = { text: string; };
            implement dispose(value: Custom): void {}
            function custom(value: &Custom): &string {
                let Custom { text } = value;
                return text;
            }
            type Nested = { inner: [bool, bool]; };
            function nested(value: &Nested): bool {
                let Nested { inner: [bit, _] } = value;
                return bit;
            }
            type Protected = { lock: LockGuard<i32>; bit: bool; };
            function protected(value: &Protected): bool {
                let Protected { bit } = value;
                return bit;
            }
            function exclusive(value: &mut Packet): bool {
                let Packet { flag } = value;
                return flag;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let check = |name| lower(name, AnalysisLimits::default());
            let forwarded = check("forward");
            let mir = forwarded
                .mir()
                .unwrap_or_else(|| panic!("forward: {:?}", forwarded.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let packet = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("Packet"))
                .unwrap();
            let field = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == loader::DeclarationKind::Field
                            && d.name.as_deref() == Some(name)
                            && packet.span.start() < d.span.start()
                            && d.span.end() < packet.span.end()
                    })
                    .unwrap()
            };
            let ops = linear(mir, mir.entry);
            assert!(
                !ops.iter()
                    .any(|op| matches!(op, Operation::Move { .. } | Operation::Dispose { .. }))
            );
            let route: Vec<_> = ops
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: incoming, into: Some(staged) },
                Operation::BindBorrowedMember { descriptor: first, projection: Projection::Field(text), into: label, binding: SharedView },
                Operation::BindBorrowedMember { descriptor: second, projection: Projection::Field(bit), into: flag, binding: CopyScalar },
                Operation::Copy { from: used, .. },
                Operation::Copy { from: view, into: Some(result) },
                Operation::Export(output),
            ] if incoming.root.span.bytes() == b"value: &Packet"
                && staged.root.node == first.root.node && staged.root.node == second.root.node
                && text.id == field("text").id && bit.id == field("flag").id
                && label.root.span.bytes() == b"label" && label.root.node == view.root.node
                && flag.root.span.bytes() == b"flag" && flag.root.node == used.root.node
                && result.root.node == output.root.node));
            let limits = AnalysisLimits {
                max_work: forwarded.statistics().work,
                max_entries: forwarded.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("forward", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = lower("forward", short);
                crate::limit::assert_body_limit(&limited);
            }
            let ignored = check("ignores");
            let mir = ignored
                .mir()
                .expect("wildcards and omissions observe no member");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::Dispose { .. }
                            | Operation::BindBorrowedMember { .. }
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Copy { from, into: Some(_) }]
                if from.root.span.bytes() == b"value: &Packet")
            );
            assert!(path_drops(mir, mir.entry).is_empty());

            for (name, owner, expected) in [
                ("copied", b"moved".as_slice(), &["omit", "skip", "text"][..]),
                (
                    "lastUse",
                    b"owner".as_slice(),
                    &["skip", "omit", "skip", "text"][..],
                ),
            ] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let fields: Vec<_> = path_drops(mir, mir.entry)
                    .into_iter()
                    .map(|drop| match drop {
                        Obligation::Drop(place) => {
                            assert_eq!(place.root.span.bytes(), owner, "{name}");
                            place.field().unwrap().name.as_deref().unwrap()
                        }
                        _ => panic!("unexpected cleanup"),
                    })
                    .collect();
                assert_eq!(fields, expected, "{name}");
            }
            for (name, code, site, related) in [
                ("escapes", "R0202", "return label;", "value: Packet"),
                ("premature", "R0203", "owner.skip = \"new\";", "&owner"),
                (
                    "expires",
                    "R0203",
                    "{ let local = value; let Packet { text: label } = &local; result = label; }",
                    "&local",
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected a single finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for (name, pattern) in [
                ("custom", "Custom { text }"),
                ("nested", "[bit, _]"),
                ("exclusive", "Packet { flag }"),
                ("protected", "Protected { bit }"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    pattern.as_bytes(),
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn owned_tuple_binding_transfers_and_cleans_initialized_elements() {
    probe(
        "patterns::owned_tuple_binding_transfers_and_cleans_initialized_elements",
        r#"
            function pass(value: [string, string, string, string]): [string, string, string, string] {
                return value;
            }
            function unpack(value: [string, string, string, string]): void {
                let [first, _, third, _] = pass(value);
            }
            function grouped(value: [string, string]): string {
                let (([((first)), (_)])) = value;
                return first;
            }
            function reuse(value: [string, string]): string {
                let [first, _] = value;
                return value[0];
            }
            function alreadyPartial(value: [string, string]): string {
                let earlier = value[1];
                let [first, _] = value;
                return first;
            }
            function copyPair(value: [bool, bool]): bool {
                let [first, _] = value;
                if (first) { return value[0]; }
                return value[1];
            }
            function forward(value: [&bool, string]): &bool {
                let [saved, _] = value;
                return saved;
            }
            function expires(flag: bool): &bool {
                let [saved, _]: [&bool, string] = [&flag, "tag"];
                return saved;
            }
            function liveAlias(p: &bool): bool {
                let mut out = p;
                { let x = true; let [r, _]: [&bool, string] = [&x, "s"]; out = r; }
                return *out;
            }
            function deadAlias(p: &bool): bool {
                let mut out = p;
                { let x = true; let [r, _]: [&bool, string] = [&x, "s"]; out = r; }
                return false;
            }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let report = lower("unpack");
            // Unchanged flat pattern budget measured in both modes at 904433f.
            let flat = AnalysisLimits {
                max_work: 7333,
                max_entries: 1200,
                ..AnalysisLimits::default()
            };
            let bounded = lower_body(receipt, module, &declaration("unpack").id, flat).unwrap();
            assert_eq!(bounded.completion(), Completion::Checked);
            assert!(bounded.mir().is_some());
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("unpack: {:?}", report.completion()));
            let mut events = Vec::new();
            let mut block = mir.entry;
            for _ in 0..mir.blocks.len() {
                let step = &mir.blocks[block];
                match &step.operation {
                    Operation::Call(_) => events.push((b"call".as_slice(), None)),
                    Operation::Move {
                        from,
                        into: Some(to),
                    } if from.tuple_element().is_some() => {
                        events.push((to.root.span.bytes(), from.tuple_element()));
                    }
                    Operation::Cleanup(cleanups) => {
                        events.extend(cleanups.iter().filter_map(|op| match op {
                            Obligation::Drop(place) => {
                                Some((place.root.span.bytes(), place.tuple_element()))
                            }
                            _ => None,
                        }))
                    }
                    _ => {}
                }
                match step.terminator {
                    Terminator::Goto(next) => block = next,
                    Terminator::Return => break,
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert!(matches!(mir.blocks[block].terminator, Terminator::Return));
            assert_eq!(
                events,
                [
                    (b"call".as_slice(), None),
                    (b"first", Some(0)),
                    (b"third", Some(2)),
                    (b"pass(value)", Some(3)),
                    (b"pass(value)", Some(1)),
                    (b"third", None),
                    (b"first", None),
                ]
            );

            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            let check =
                |limits| lower_body(receipt, module, &declaration("unpack").id, limits).unwrap();
            assert!(check(limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = check(short);
                crate::limit::assert_body_limit(&limited);
            }

            for (name, site, source, code) in [
                (
                    "reuse",
                    b"value[0]".as_slice(),
                    b"value".as_slice(),
                    "R0200",
                ),
                (
                    "alreadyPartial",
                    b"value".as_slice(),
                    b"value[1]".as_slice(),
                    "R0200",
                ),
                (
                    "expires",
                    b"return saved;".as_slice(),
                    b"flag: bool".as_slice(),
                    "R0202",
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site, "{name}");
                assert_eq!(finding.related.span.bytes(), source, "{name}");
            }

            let grouped = lower("grouped");
            let mir = grouped
                .mir()
                .unwrap_or_else(|| panic!("grouped: {:?}", grouped.completion()));
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Move { from, into: Some(to) }
                if from.tuple_element() == Some(0) && to.root.span.bytes() == b"first")
            ));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.tuple_element() == Some(1))
            );

            let live = lower("liveAlias");
            assert_eq!(live.completion(), Completion::SourceRejected);
            assert!(live.mir().is_none());
            let [finding] = live.findings() else {
                panic!("expected one live-alias finding")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(
                finding.site.span.bytes(),
                b"{ let x = true; let [r, _]: [&bool, string] = [&x, \"s\"]; out = r; }"
            );
            assert_eq!(finding.related.span.bytes(), b"&x");
            let dead = lower("deadAlias");
            let mir = dead.mir().expect("dead alias does not block cleanup");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.tuple_element() == Some(1))
            );

            let report = lower("copyPair");
            let mir = report.mir().expect("copyable tuple stays initialized");
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Copy { from, into: Some(to) }
                if from.tuple_element() == Some(0) && to.root.span.bytes() == b"first")
            ));
            let report = lower("forward");
            let mir = report
                .mir()
                .expect("return caller backing, not a loan of the scratch tuple");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.tuple_element() == Some(1))
            );
        },
    );
}

#[test]
fn fresh_exclusive_tuple_pattern_keeps_the_original_restriction() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, ExclusiveView},
        Projection,
    };

    probe(
        "patterns::fresh_exclusive_tuple_pattern_keeps_the_original_restriction",
        r#"
            function copies(seed: [bool, bool, string]): bool {
                let mut owner = seed;
                let ([(first), second, _]) = (&mut owner);
                let _ = first;
                let moved = owner;
                return second;
            }
            function touch(view: &mut string): void {}
            function resumes(seed: [bool, string]): bool {
                let mut owner = seed;
                let [_, view] = &mut owner;
                touch(view);
                owner[1] = "new";
                return owner[0];
            }
            function conflicts(seed: [bool, string]): bool {
                let mut owner = seed;
                let [_, view] = &mut owner;
                let moved = view;
                let bit = owner[0];
                let _ = moved;
                return bit;
            }
            function heldReborrow(seed: [bool, string]): bool {
                let mut owner = seed;
                let [_, view] = &mut owner;
                let child = &mut *view;
                let bit = owner[0];
                let _ = child;
                return bit;
            }
            function suspendedView(seed: [bool, string]): void {
                let mut owner = seed;
                let [_, view] = &mut owner;
                let child = &mut *view;
                touch(view);
                touch(child);
            }
            function acquisition(seed: [bool, string]): bool {
                let mut owner = seed;
                let shared = &owner;
                let [bit, _] = &mut owner;
                let _ = shared;
                return bit;
            }
            function replaceEarly(seed: [bool, string]): void {
                let mut owner = seed;
                let [_, view] = &mut owner;
                owner[0] = true;
                let _ = view;
            }
            function escapes(seed: [bool, string]): &mut string {
                let mut owner = seed;
                let [_, view] = &mut owner;
                return view;
            }
            function expires(seed: &mut string, input: [bool, string], branch: bool): void {
                let mut out = seed;
                if (branch) { let mut owner = input; let [_, view] = &mut owner; out = view; }
                let _ = out;
            }
            function named(seed: [bool, string]): void {
                let mut owner = seed;
                let parent = &mut owner;
                let [_, view] = parent;
                let _ = view;
            }
            function both(seed: [string, string]): void {
                let mut owner = seed;
                let [first, second] = &mut owner;
                let _ = first;
                let _ = second;
            }
            function mixed(seed: [bool, string]): void {
                let mut owner = seed;
                let [flag, view] = &mut owner;
                let _ = flag;
                let _ = view;
            }
            type Wrapper = { pair: [bool, string]; };
            function indirect(input: &mut Wrapper): bool {
                let [flag, _] = &mut input.pair;
                return flag;
            }
            function carried(seed: [&bool, string]): &bool {
                let mut owner = seed;
                let [saved, _] = &mut owner;
                return saved;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let check = |name| lower(name, AnalysisLimits::default());
            let copies = check("copies");
            let mir = copies
                .mir()
                .unwrap_or_else(|| panic!("copies: {:?}", copies.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.root.span.bytes() == b"moved" && place.tuple_element() == Some(2))
            );
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Borrow { .. }
                    | Operation::BindBorrowedMember { .. }
                    | Operation::Export(_) => true,
                    Operation::Move { into: Some(to), .. } => to.root.span.bytes() == b"moved",
                    Operation::Copy { from, .. } => from.root.span.bytes() == b"second",
                    _ => false,
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source, holder, exclusive: true },
                Operation::BindBorrowedMember { descriptor: a, projection: Projection::TupleElement(0), into: first, binding: CopyScalar },
                Operation::BindBorrowedMember { descriptor: b, projection: Projection::TupleElement(1), into: second, binding: CopyScalar },
                Operation::Move { from: owner, .. },
                Operation::Copy { from: read, into: Some(result) },
                Operation::Export(output),
            ] if source.projection.is_none() && source.root.node == owner.root.node
                && holder.root.node == a.root.node && holder.root.node == b.root.node
                && first.root.span.bytes() == b"first" && second.root.span.bytes() == b"second"
                && read.root.node == second.root.node && output.root.node == result.root.node));
            let resumes = check("resumes");
            let mir = resumes
                .mir()
                .expect("owner assignment resumes after the view's last use");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Reborrow { .. }
                            | Operation::Call(_)
                            | Operation::Install { .. }
                    )
                })
                .collect();
            let touch = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("touch"))
                .unwrap();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source, holder, exclusive: true },
                Operation::BindBorrowedMember { descriptor, projection: Projection::TupleElement(1), into: view, binding: ExclusiveView },
                Operation::Reborrow { parent, exclusive: true, .. },
                Operation::Call(callee),
                Operation::Install { target, .. },
            ] if source.projection.is_none() && source.root.node == target.root.node && target.tuple_element() == Some(1)
                && holder.root.node == descriptor.root.node && parent.root.node == view.root.node && **callee == touch.id));
            let limits = AnalysisLimits {
                max_work: resumes.statistics().work,
                max_entries: resumes.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("resumes", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = lower("resumes", short);
                crate::limit::assert_body_limit(&limited);
            }
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(old), Obligation::Drop(replacement)]
                if old.root.node == replacement.root.node && old.root.span.bytes() == b"owner"
                    && old.tuple_element() == Some(1) && replacement.tuple_element() == Some(1))
            );
            for (name, code, at, cause) in [
                ("conflicts", "R0201", "owner[0]", "&mut owner"),
                ("heldReborrow", "R0201", "owner[0]", "&mut owner"),
                ("suspendedView", "R0201", "view", "&mut *view"),
                ("acquisition", "R0201", "&mut owner", "&owner"),
                ("replaceEarly", "R0203", "owner[0] = true;", "&mut owner"),
                ("escapes", "R0202", "return view;", "owner"),
                (
                    "expires",
                    "R0203",
                    "{ let mut owner = input; let [_, view] = &mut owner; out = view; }",
                    "&mut owner",
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), at.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), cause.as_bytes(), "{name}");
            }
            for (name, pattern) in [
                ("named", "[_, view]"),
                ("both", "[first, second]"),
                ("mixed", "[flag, view]"),
                ("indirect", "[flag, _]"),
                ("carried", "[saved, _]"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    pattern.as_bytes(),
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn explicit_exclusive_tuple_pattern_reborrow_suspends_the_named_parent() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, ExclusiveView},
        Projection,
    };

    probe(
        "patterns::explicit_exclusive_tuple_pattern_reborrow_suspends_the_named_parent",
        r#"
            function touch(view: &mut string): void {}
            function resumes(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let [_, view] = &mut *(parent);
                touch(view);
                let [bit, _] = &mut *parent;
                let _ = parent;
                owner[1] = "replacement";
                return bit;
            }
            function suspended(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let [_, view] = &mut *parent;
                let moved = view;
                let [bit, _] = &mut *(parent);
                touch(moved);
                return bit;
            }
            function ownerStillBorrowed(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let [_, view] = &mut *parent;
                let bit = owner[0];
                touch(view);
                return bit;
            }
            function localEscape(seed: [bool, string]): &mut string {
                let mut owner = seed;
                let parent = &mut owner;
                let [_, view] = &mut *parent;
                return view;
            }
            function incoming(parent: &mut [bool, string]): &mut string {
                let [_, view] = &mut *parent;
                return view;
            }
            function holderEnds(seed: [bool, string], fallback: &mut string): bool {
                let mut owner = seed;
                let mut out = fallback;
                { let parent = &mut owner; let [_, view] = &mut *parent; out = view; }
                touch(out);
                owner[1] = "replacement";
                return owner[0];
            }
            function holderEndsEarly(seed: [bool, string], fallback: &mut string): bool {
                let mut owner = seed;
                let mut out = fallback;
                { let parent = &mut owner; let [_, view] = &mut *parent; out = view; }
                let bit = owner[0];
                touch(out);
                return bit;
            }
            function backingEnds(seed: [bool, string], fallback: &mut string, branch: bool): void {
                let mut out = fallback;
                if (branch) { let mut owner = seed; let parent = &mut owner; let [_, view] = &mut *parent; out = view; }
                touch(out);
            }
            function movedParent(seed: [bool, string]): void {
                let mut owner = seed;
                let parent = &mut owner;
                let moved = parent;
                let [_, view] = &mut *parent;
                touch(view);
            }
            function blockedMove(seed: [bool, string]): void {
                let mut owner = seed;
                let parent = &mut owner;
                let [_, view] = &mut *parent;
                let moved = parent;
                touch(view);
            }
            function indirect(parent: &mut &mut [bool, string]): bool {
                let [bit, _] = &mut **parent;
                return bit;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let check = |name| lower(name, AnalysisLimits::default());
            let resumes = check("resumes");
            let mir = resumes
                .mir()
                .unwrap_or_else(|| panic!("resumes: {:?}", resumes.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(old), Obligation::Drop(replacement)]
                if old.root.node == replacement.root.node && old.root.span.bytes() == b"owner"
                    && old.tuple_element() == Some(1) && replacement.tuple_element() == Some(1))
            );
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. }
                            | Operation::Reborrow { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Call(_)
                            | Operation::Install { .. }
                    )
                })
                .collect();
            let touch = module
                .declarations
                .iter()
                .find(|decl| decl.name.as_deref() == Some("touch"))
                .unwrap();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source: owner, holder: parent, exclusive: true },
                Operation::Reborrow { parent: before, holder: staged, exclusive: true },
                Operation::BindBorrowedMember { descriptor, projection: Projection::TupleElement(1), into: view, binding: ExclusiveView },
                Operation::Reborrow { parent: used, exclusive: true, .. },
                Operation::Call(callee),
                Operation::Reborrow { parent: resumed, holder: scalar_stage, exclusive: true },
                Operation::BindBorrowedMember { descriptor: scalar, projection: Projection::TupleElement(0), binding: CopyScalar, .. },
                Operation::Install { target, .. },
            ] if owner.projection.is_none() && owner.root.node == target.root.node && target.tuple_element() == Some(1)
                && before.root.node == parent.root.node && resumed.root.node == parent.root.node
                && staged.root.node == descriptor.root.node && scalar_stage.root.node == scalar.root.node
                && used.root.node == view.root.node && **callee == touch.id));
            let limits = AnalysisLimits {
                max_work: resumes.statistics().work,
                max_entries: resumes.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("resumes", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = lower("resumes", short);
                crate::limit::assert_body_limit(&limited);
            }
            let incoming = check("incoming");
            let mir = incoming
                .mir()
                .expect("a view of incoming backing may be returned");
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Move { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Reborrow { parent, holder: staged, exclusive: true },
                Operation::BindBorrowedMember { descriptor, projection: Projection::TupleElement(1), into: view, binding: ExclusiveView },
                Operation::Move { from, into: Some(result) },
                Operation::Export(output),
            ] if parent.root.span.bytes() == b"parent: &mut [bool, string]" && staged.root.node == descriptor.root.node
                && view.root.node == from.root.node && result.root.node == output.root.node));
            let holder_ends = check("holderEnds");
            assert!(
                holder_ends.mir().is_some(),
                "closing the parent slot must not end its original backing: {:?}",
                holder_ends.completion()
            );
            for (name, code, at, cause) in [
                ("suspended", "R0201", "&mut *(parent)", "&mut *parent"),
                ("ownerStillBorrowed", "R0201", "owner[0]", "&mut owner"),
                ("holderEndsEarly", "R0201", "owner[0]", "&mut owner"),
                ("localEscape", "R0202", "return view;", "owner"),
                (
                    "backingEnds",
                    "R0203",
                    "{ let mut owner = seed; let parent = &mut owner; let [_, view] = &mut *parent; out = view; }",
                    "&mut owner",
                ),
                ("movedParent", "R0200", "&mut *parent", "parent"),
                ("blockedMove", "R0203", "parent", "&mut *parent"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), at.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), cause.as_bytes(), "{name}");
            }
            let indirect = check("indirect");
            assert_eq!(indirect.completion(), Completion::Incomplete);
            assert!(indirect.mir().is_none());
            assert_eq!(indirect.blocked_at().unwrap().span.bytes(), b"[bit, _]");
        },
    );
}

#[test]
fn shared_product_binding_copies_carried_references_not_container_access() {
    use compiler::analysis::ownership::{BorrowedMemberBinding::CopyReference, Projection};

    probe(
        "patterns::shared_product_binding_copies_carried_references_not_container_access",
        r#"
            type Carrier = { saved: &bool; other: &bool; bit: bool; tag: string; };
            function tupleForward(seed: &bool, tag: string): &bool {
                let pair: [&bool, string] = [seed, tag];
                let alias = &pair;
                let [(view), _] = alias;
                let moved = pair;
                return view;
            }
            function recordForward(seed: &bool, tag: string): &bool {
                let value = Carrier { saved: seed, other: seed, bit: true, tag: tag };
                let Carrier { saved: view } = &value;
                let moved = value;
                return view;
            }
            function throughParameter(value: &Carrier): &bool {
                let Carrier { saved } = value;
                return saved;
            }
            function siblingBacking(seed: &bool, flag: bool): &bool {
                let value = Carrier { saved: seed, other: &flag, bit: false, tag: "other" };
                let Carrier { saved: view } = &value;
                let moved = value;
                return view;
            }
            function pointsIntoContainer(seed: &bool): &bool {
                let mut value = Carrier { saved: seed, other: seed, bit: true, tag: "local" };
                value.saved = &value.bit;
                let Carrier { saved: view } = &value;
                return view;
            }
            function sourceExpires(seed: &bool, branch: bool): bool {
                let mut out = seed;
                if (branch) {
                    let flag = true;
                    let pair: [&bool, string] = [&flag, "local"];
                    let [view, _] = &pair;
                    out = view;
                }
                return *out;
            }
            function survivesMove(flag: bool, tag: string): bool {
                let pair: [&bool, string] = [&flag, tag];
                let [view, _] = &pair;
                let moved = pair;
                return *view;
            }
            function carry(seed: &bool): [&bool, string] { return [seed, "callee"]; }
            function rootBacking(flag: bool): &bool {
                let pair = carry(&flag);
                let [view, _] = &pair;
                return view;
            }
            function suspended(seed: bool): bool {
                let mut source = seed;
                let access = &mut source;
                let first = &*access;
                let pair: [&bool, string] = [first, "retained"];
                let [view, _] = &pair;
                let _ = *access;
                return *view;
            }
            function exclusiveBacking(seed: bool): bool {
                let mut source = seed;
                let access = &mut source;
                let first = &*access;
                let pair: [&bool, string] = [first, "retained"];
                let [view, _] = &pair;
                let _ = source;
                return *view;
            }
            function projectedStringStillBorrowsOwner(seed: &bool): &string {
                let pair: [&bool, string] = [seed, "local"];
                let [view, text] = &pair;
                let _ = view;
                return text;
            }
            function exclusiveMember(value: &[&mut bool, bool]): bool {
                let [_, flag] = value;
                return flag;
            }
            function nestedReference(value: &[&&bool, bool]): bool {
                let [_, flag] = value;
                return flag;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let check = |name| lower(name, AnalysisLimits::default());
            for name in ["tupleForward", "recordForward", "throughParameter"] {
                let report = check(name);
                let mir = report.mir().unwrap_or_else(|| {
                    panic!(
                        "{name}: {:?} at {:?}",
                        report.completion(),
                        report
                            .blocked_at()
                            .map(|site| std::str::from_utf8(site.span.bytes()).unwrap())
                    )
                });
                let route: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| match op {
                        Operation::Borrow { .. }
                        | Operation::BindBorrowedMember { .. }
                        | Operation::Export(_) => true,
                        Operation::Move {
                            into: Some(into), ..
                        } => into.root.span.bytes() == b"moved",
                        Operation::Copy { from, .. } => matches!(
                            from.root.span.bytes(),
                            b"alias" | b"value: &Carrier" | b"view" | b"saved"
                        ),
                        _ => false,
                    })
                    .collect();
                let bind = route
                    .iter()
                    .position(|op| matches!(op, Operation::BindBorrowedMember { .. }))
                    .unwrap();
                let (stage, uses) = route.split_at(bind);
                let [
                    Operation::BindBorrowedMember {
                        descriptor,
                        projection,
                        into: bound,
                        binding: CopyReference,
                    },
                    remaining @ ..,
                ] = uses
                else {
                    panic!("{name}: expected a stored reference copy")
                };
                let owner = match (name, stage) {
                    (
                        "tupleForward",
                        [
                            Operation::Borrow {
                                source,
                                holder,
                                exclusive: false,
                            },
                            Operation::Copy {
                                from: alias,
                                into: Some(staged),
                            },
                        ],
                    ) => {
                        assert!(matches!(projection, Projection::TupleElement(0)));
                        assert_eq!(source.root.span.bytes(), b"pair");
                        assert_eq!(alias.root.node, holder.root.node);
                        assert_eq!(staged.root.node, descriptor.root.node);
                        Some(source)
                    }
                    (
                        "recordForward",
                        [
                            Operation::Borrow {
                                source,
                                holder,
                                exclusive: false,
                            },
                        ],
                    ) => {
                        assert_eq!(source.root.span.bytes(), b"value");
                        assert_eq!(holder.root.node, descriptor.root.node);
                        Some(source)
                    }
                    (
                        "throughParameter",
                        [
                            Operation::Copy {
                                into: Some(staged), ..
                            },
                        ],
                    ) => {
                        assert_eq!(staged.root.node, descriptor.root.node);
                        None
                    }
                    _ => panic!("{name}: expected the original descriptor stage"),
                };
                let field = |name| {
                    module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.id.kind == loader::DeclarationKind::Field
                                && d.name.as_deref() == Some(name)
                        })
                        .unwrap()
                };
                if name != "tupleForward" {
                    assert!(
                        matches!(projection, Projection::Field(declaration) if declaration.id == field("saved").id)
                    );
                }
                let rest = if let Some(owner) = owner {
                    let [
                        Operation::Move {
                            from,
                            into: Some(destination),
                        },
                        rest @ ..,
                    ] = remaining
                    else {
                        panic!("{name}: expected a move after binding")
                    };
                    assert_eq!(from.root.node, owner.root.node);
                    let drops = path_drops(mir, mir.entry);
                    let [Obligation::Drop(place)] = drops.as_slice() else {
                        panic!("{name}: expected one cleanup")
                    };
                    assert_eq!(place.root.node, destination.root.node);
                    if name == "tupleForward" {
                        assert_eq!(place.tuple_element(), Some(1));
                    } else {
                        assert!(
                            place
                                .field()
                                .is_some_and(|member| member.id == field("tag").id)
                        );
                    }
                    rest
                } else {
                    assert!(path_drops(mir, mir.entry).is_empty());
                    remaining
                };
                assert!(
                    matches!(rest, [Operation::Copy { from, into: Some(result) }, Operation::Export(output)]
                    if from.root.node == bound.root.node && result.root.node == output.root.node)
                );
            }
            let moved = check("survivesMove");
            let mir = moved
                .mir()
                .expect("the stored local reference survives the container move");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::BindBorrowedMember { .. }
                    | Operation::Dereference(_)
                    | Operation::Export(_) => true,
                    Operation::Move {
                        into: Some(into), ..
                    } => into.root.span.bytes() == b"moved",
                    _ => false,
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::BindBorrowedMember { into: view, binding: CopyReference, .. },
                Operation::Move { from, into: Some(owner) },
                Operation::Dereference(read),
                Operation::Export(_),
            ] if view.root.node == read.root.node && from.root.span.bytes() == b"pair" && owner.root.span.bytes() == b"moved"));
            let forward = check("tupleForward");
            let limits = AnalysisLimits {
                max_work: forward.statistics().work,
                max_entries: forward.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("tupleForward", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = lower("tupleForward", short);
                crate::limit::assert_body_limit(&limited);
            }
            for (name, code, site, related) in [
                ("siblingBacking", "R0202", "return view;", "flag: bool"),
                ("pointsIntoContainer", "R0202", "return view;", "&value.bit"),
                (
                    "sourceExpires",
                    "R0203",
                    "{\n                    let flag = true;",
                    "&flag",
                ),
                ("rootBacking", "R0202", "return view;", "flag: bool"),
                ("suspended", "R0201", "*access", "&*access"),
                ("exclusiveBacking", "R0201", "source", "&mut source"),
                (
                    "projectedStringStillBorrowsOwner",
                    "R0202",
                    "return text;",
                    "pair",
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                if name == "sourceExpires" {
                    assert!(
                        finding.site.span.bytes().starts_with(site.as_bytes()),
                        "{name}"
                    );
                } else {
                    assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                }
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for name in ["exclusiveMember", "nestedReference"] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), b"[_, flag]");
            }
        },
    );
}

#[test]
fn nested_exclusive_patterns_keep_the_incoming_parent_suspended() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, ExclusiveView},
        Projection,
    };

    probe(
        "patterns::nested_exclusive_patterns_keep_the_incoming_parent_suspended",
        r#"
            function touch(view: &mut string): void {}
            function incoming(parent: &mut [bool, [i32, string]]): &mut string {
                let [_, ([_, view])] = &mut *(parent);
                return view;
            }
            function resumes(parent: &mut [[bool, string], [bool, i32]]): bool {
                let [[_, view], _] = &mut *parent;
                touch(view);
                let [_, [bit, number]]: &mut [[bool, string], [bool, i32]] = &mut *(parent);
                let moved = parent;
                return bit;
            }
            function allWildcards(parent: &mut [[bool, string], i32]): void {
                let [[_, _], _] = (&mut *(parent));
                let moved = parent;
            }
            function blocked(parent: &mut [[bool, string], [bool, i32]]): bool {
                let [[_, view], _] = &mut *parent;
                let viewMoved = view;
                let [_, [bit, _]] = &mut *(parent);
                touch(viewMoved);
                return bit;
            }
            function blockedMove(parent: &mut [[bool, string], i32]): void {
                let [[_, view], _] = &mut *parent;
                let moved = parent;
                touch(view);
            }
            function movedParent(parent: &mut [[bool, string], i32]): void {
                let moved = parent;
                let [[_, view], _] = &mut *parent;
                touch(view);
            }
            function bare(parent: &mut [[bool, string], i32]): &mut string {
                let [[_, view], _] = parent;
                return view;
            }
            function expressionOnly(parent: &mut [[bool, string], i32]): void {
                let alias = &mut *parent;
            }
            function localHolder(parent: &mut [[bool, string], i32]): &mut string {
                let alias = parent;
                let [[_, view], _] = &mut *alias;
                return view;
            }
            function referenceLeaf(parent: &mut [[bool, &string], i32]): bool {
                let [[bit, _], _] = &mut *parent;
                return bit;
            }
            function wholeMember(parent: &mut [[bool, string], i32]): void {
                let [member, _] = &mut *parent;
            }
            function deeper(parent: &mut [[[bool, string]], i32]): void {
                let [[[_, view]], _] = &mut *parent;
                touch(view);
            }
            function multiple(parent: &mut [[bool, string], i32]): bool {
                let [[bit, view], _] = &mut *parent;
                touch(view);
                return bit;
            }
        "#,
        |_, module, receipt| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let incoming = lower("incoming", default);
            let mir = incoming
                .mir()
                .unwrap_or_else(|| panic!("nested incoming: {:?}", incoming.completion()));
            let ops = linear(mir, mir.entry);
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Move { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            let [
                Operation::Reborrow {
                    parent,
                    holder,
                    exclusive: true,
                },
                Operation::BindBorrowedMember {
                    descriptor,
                    projection: Projection::NestedTupleElement { outer: 1, inner: 1 },
                    into: view,
                    binding: ExclusiveView,
                },
                Operation::Move {
                    from,
                    into: Some(staged),
                },
                Operation::Export(output),
            ] = route.as_slice()
            else {
                panic!("unexpected nested return route")
            };
            assert_eq!(
                parent.root.span.bytes(),
                b"parent: &mut [bool, [i32, string]]"
            );
            assert_eq!(descriptor.root.node, holder.root.node);
            assert_eq!(view.root.node, from.root.node);
            assert_eq!(staged.root.node, output.root.node);
            let resumes = lower("resumes", default);
            let mir = resumes
                .mir()
                .expect("the same incoming parent resumes after the leaf's last use");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Call(_)
                            | Operation::Move { .. }
                            | Operation::Copy { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Reborrow { parent, holder, exclusive: true },
                Operation::BindBorrowedMember { descriptor: view_stage, projection: Projection::NestedTupleElement { outer: 0, inner: 1 }, into: view, binding: ExclusiveView },
                Operation::Reborrow { parent: passed, exclusive: true, .. },
                Operation::Call(callee),
                Operation::Reborrow { parent: resumed, holder: scalar_stage, exclusive: true },
                Operation::BindBorrowedMember { descriptor: scalar_a, projection: Projection::NestedTupleElement { outer: 1, inner: 0 }, into: bit, binding: CopyScalar },
                Operation::BindBorrowedMember { descriptor: scalar_b, projection: Projection::NestedTupleElement { outer: 1, inner: 1 }, binding: CopyScalar, .. },
                Operation::Move { from: original, into: Some(moved) },
                Operation::Copy { from: returned, into: Some(result) },
                Operation::Export(export),
            ] if holder.root.node == view_stage.root.node && passed.root.node == view.root.node && *callee == id("touch")
                && parent.root.node == resumed.root.node && parent.root.node == original.root.node
                && scalar_stage.root.node == scalar_a.root.node && scalar_stage.root.node == scalar_b.root.node
                && moved.root.span.bytes() == b"moved" && bit.root.node == returned.root.node && result.root.node == export.root.node));
            let wildcards = lower("allWildcards", default);
            let mir = wildcards
                .mir()
                .expect("wildcards end the stage without binding any leaf");
            let ops = linear(mir, mir.entry);
            assert!(path_drops(mir, mir.entry).is_empty());
            assert!(
                !ops.iter()
                    .any(|op| matches!(op, Operation::BindBorrowedMember { .. }))
            );
            let route: Vec<_> = ops
                .into_iter()
                .filter(|op| matches!(op, Operation::Reborrow { .. } | Operation::Move { .. }))
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Reborrow { parent, holder, exclusive: true }, Operation::Move { from, .. }]
                if parent.root.node == from.root.node && holder.root.span.bytes() == b"(&mut *(parent))")
            );
            for (name, code, site, subject) in [
                (
                    "blocked",
                    "R0201",
                    b"&mut *(parent)".as_slice(),
                    b"&mut *parent".as_slice(),
                ),
                ("blockedMove", "R0203", b"parent", b"&mut *parent"),
                ("movedParent", "R0200", b"&mut *parent", b"parent"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), subject);
            }
            for (name, site) in [
                ("bare", b"[[_, view], _]".as_slice()),
                ("expressionOnly", b"*parent"),
                ("localHolder", b"[[_, view], _]"),
                ("referenceLeaf", b"[[bit, _], _]"),
                ("wholeMember", b"[member, _]"),
                ("deeper", b"[[[_, view]], _]"),
                ("multiple", b"[[bit, view], _]"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site, "{name}");
            }
            let exact = AnalysisLimits {
                max_work: resumes.statistics().work,
                max_entries: resumes.statistics().entries,
                max_depth: resumes.statistics().max_depth,
            };
            assert!(lower("resumes", exact).mir().is_some());
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
                let report = lower("resumes", short);
                crate::limit::assert_body_limit(&report);
            }
        },
    );
}

#[test]
fn owned_nested_tuple_patterns_transfer_leaves_and_clean_the_temporary() {
    use compiler::analysis::ownership::Projection;

    probe(
        "patterns::owned_nested_tuple_patterns_transfer_leaves_and_clean_the_temporary",
        r#"
            function untouched(value: [[string, bool], string, [bool, string]]): void {}
            function unpack(value: [[string, bool, string], string, [string, string], string]): string {
                let ([([_, bit, (selected)]), tail, [_, _], _]) = (value);
                return selected;
            }
            function reuse(value: [[string, string], bool]): bool {
                let [[_, text], _] = value;
                let [[_, again], bit] = value;
                return bit;
            }
            function copied(value: [[bool, bool], bool, [bool, bool]]): bool {
                let [[_, first], _, [_, other]] = value;
                let [[_, _], again, [_, _]] = value;
                return again;
            }
            function depthBare(value: [[bool, bool], bool]): void { let [[_, _], _] = value; }
            function depthGrouped(value: [[bool, bool], bool]): void { let [[_, _], _] = ((value)); }
            function ignore(value: [[string, string], string]): void {
                let [[_, _], _] = value;
            }
            function child(value: [[string, bool], string, [bool, string]]): string {
                let [whole, _, [_, text]] = value;
                return text;
            }
            function forward(value: [[bool, bool], bool]): [[bool, bool], bool] { return value; }
            function temporary(value: [[bool, bool], bool]): bool {
                let [[flag, _], _] = forward(value);
                return flag;
            }
            function indexed(value: [[bool, bool], bool]): bool { return value[1]; }
            function deeper(value: [[[bool, string], bool], bool]): bool {
                let [[[bit, _], _], _] = value;
                return bit;
            }
            function reference(value: [[&bool, bool], bool]): &bool {
                let [[saved, _], _] = value;
                return saved;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &declaration.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            let unpack = lower("unpack", default);
            let mir = unpack.mir().unwrap_or_else(|| {
                panic!(
                    "owned nested tuple: {:?} at {:?}",
                    unpack.completion(),
                    unpack.blocked_at().map(|site| site.span.bytes())
                )
            });
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::Copy { .. }
                            | Operation::Cleanup(_)
                            | Operation::Drop(_)
                            | Operation::Export(_)
                    )
                })
                .collect();
            let [
                Operation::Move {
                    from: value,
                    into: Some(scratch),
                },
                Operation::Copy {
                    from: flag,
                    into: Some(bit),
                },
                Operation::Move {
                    from: leaf,
                    into: Some(selected),
                },
                Operation::Move {
                    from: sibling,
                    into: Some(tail),
                },
                Operation::Cleanup(remaining),
                Operation::Move {
                    from: result,
                    into: Some(staged),
                },
                Operation::Export(output),
                Operation::Cleanup(lexical),
                Operation::Cleanup(parameters),
            ] = route.as_slice()
            else {
                panic!(
                    "expected one stage, leaf transfers, temporary cleanup, then export: {route:?}"
                );
            };
            assert_eq!(
                value.root.span.bytes(),
                b"value: [[string, bool, string], string, [string, string], string]"
            );
            assert_eq!(scratch.root.span.bytes(), b"(value)");
            assert!(value.projection.is_none() && scratch.projection.is_none());
            let nested = |place: &compiler::analysis::ownership::Place<'_>, a, b| {
                place.root.node == scratch.root.node
                    && matches!(place.projection, Some(Projection::NestedTupleElement { outer, inner }) if (outer, inner) == (a, b))
            };
            assert!(nested(flag, 0, 1) && nested(leaf, 0, 2));
            assert_eq!(sibling.root.node, scratch.root.node);
            assert_eq!(sibling.tuple_element(), Some(1));
            assert_eq!(bit.root.span.bytes(), b"bit");
            assert_eq!(selected.root.span.bytes(), b"selected");
            assert_eq!(tail.root.span.bytes(), b"tail");
            assert_eq!(selected.root.node, result.root.node);
            assert_eq!(staged.root.node, output.root.node);
            assert!(
                matches!(remaining.as_slice(), [Obligation::Drop(last), Obligation::Drop(right), Obligation::Drop(left), Obligation::EndStorage(second), Obligation::EndStorage(flag), Obligation::Drop(first), Obligation::EndStorage(child), Obligation::EndStorage(root)]
                if last.root.node == scratch.root.node && last.tuple_element() == Some(3)
                    && nested(right, 2, 1) && nested(left, 2, 0) && second.root.node == scratch.root.node && second.tuple_element() == Some(2)
                    && nested(flag, 0, 1) && nested(first, 0, 0) && child.root.node == scratch.root.node && child.tuple_element() == Some(0)
                    && root.root.node == scratch.root.node && root.projection.is_none())
            );
            assert!(
                matches!(lexical.as_slice(), [Obligation::Drop(name), Obligation::EndStorage(flag)]
                if name.root.node == tail.root.node && flag.root.node == bit.root.node
                    && name.projection.is_none() && flag.projection.is_none())
            );
            assert!(parameters.is_empty());
            let untouched = lower("untouched", default);
            let mir = untouched
                .mir()
                .expect("an untouched parameter also cleans its children");
            let ops: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Cleanup(_) | Operation::Move { .. } | Operation::Drop(_)
                    )
                })
                .collect();
            let [Operation::Cleanup(cleanup)] = ops.as_slice() else {
                panic!("expected only parameter cleanup")
            };
            let parts: Vec<_> = cleanup
                .iter()
                .map(|action| {
                    let (drop, place) = match action {
                        Obligation::Drop(place) => (true, place),
                        Obligation::EndStorage(place) => (false, place),
                        _ => panic!("expected only default cleanup"),
                    };
                    assert_eq!(
                        place.root.span.bytes(),
                        b"value: [[string, bool], string, [bool, string]]"
                    );
                    (
                        drop,
                        match place.projection {
                            None => (None, None),
                            Some(Projection::TupleElement(outer)) => (Some(outer), None),
                            Some(Projection::NestedTupleElement { outer, inner }) => {
                                (Some(outer), Some(inner))
                            }
                            _ => panic!("expected a tuple position"),
                        },
                    )
                })
                .collect();
            assert_eq!(
                parts,
                [
                    (true, (Some(2), Some(1))),
                    (false, (Some(2), Some(0))),
                    (false, (Some(2), None)),
                    (true, (Some(1), None)),
                    (false, (Some(0), Some(1))),
                    (true, (Some(0), Some(0))),
                    (false, (Some(0), None)),
                    (false, (None, None)),
                ]
            );
            let ignored = lower("ignore", default);
            let mir = ignored
                .mir()
                .expect("a wildcard-only nested pattern still stages the source");
            let moves: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::Copy { .. }))
                .collect();
            let [
                Operation::Move {
                    from,
                    into: Some(scratch),
                },
            ] = moves.as_slice()
            else {
                panic!("expected just the wildcard temporary move")
            };
            assert_eq!(from.root.span.bytes(), b"value: [[string, string], string]");
            assert!(from.projection.is_none() && scratch.projection.is_none());
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(last), Obligation::Drop(right), Obligation::Drop(left)]
                if [last, right, left].iter().all(|place| place.root.node == scratch.root.node)
                    && last.tuple_element() == Some(1)
                    && matches!(right.projection, Some(Projection::NestedTupleElement { outer: 0, inner: 1 }))
                    && matches!(left.projection, Some(Projection::NestedTupleElement { outer: 0, inner: 0 })))
            );
            let reused = lower("reuse", default);
            assert_eq!(reused.completion(), Completion::SourceRejected);
            assert!(reused.mir().is_none());
            let [finding] = reused.findings() else {
                panic!("expected one moved source finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"value");
            assert_eq!(finding.related.span.bytes(), b"value");
            assert!(finding.related.span.start() < finding.site.span.start());
            let copied = lower("copied", default);
            let mir = copied.mir().expect("a copyable source can be staged twice");
            let copies: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter_map(|op| match op {
                    Operation::Move { .. } => panic!("an all-scalar tuple does not move"),
                    Operation::Copy {
                        from,
                        into: Some(dest),
                    } => Some((from, dest)),
                    _ => None,
                })
                .collect();
            assert!(
                matches!(copies.as_slice(), [(source, first), (leaf, binder), (peer, other), (reused, second), (sibling, again), (returned, _)]
                if source.root.span.bytes() == b"value: [[bool, bool], bool, [bool, bool]]" && source.root.node == reused.root.node
                    && first.root.node != second.root.node && first.root.node == leaf.root.node && first.root.node == peer.root.node
                    && matches!(leaf.projection, Some(Projection::NestedTupleElement { outer: 0, inner: 1 }))
                    && matches!(peer.projection, Some(Projection::NestedTupleElement { outer: 2, inner: 1 }))
                    && binder.root.span.bytes() == b"first" && other.root.span.bytes() == b"other"
                    && second.root.node == sibling.root.node && sibling.tuple_element() == Some(1)
                    && again.root.span.bytes() == b"again" && again.root.node == returned.root.node)
            );
            assert!(path_drops(mir, mir.entry).is_empty());
            let indexed = lower("indexed", default);
            let mir = indexed
                .mir()
                .expect("the exact outer scalar leaf can be copied");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::Move { .. } | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Copy { from, into: Some(staged) }, Operation::Export(output)]
                if from.root.span.bytes() == b"value: [[bool, bool], bool]" && from.tuple_element() == Some(1)
                    && staged.root.node == output.root.node && staged.projection.is_none() && output.projection.is_none())
            );
            assert!(path_drops(mir, mir.entry).is_empty());
            let child = lower("child", default);
            let mir = child.mir().expect("a whole child is a standalone owner");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(remainder), Obligation::Drop(whole)]
                if remainder.tuple_element() == Some(1) && whole.root.span.bytes() == b"whole"
                && whole.tuple_element() == Some(0))
            );
            for (name, at) in [
                ("temporary", b"forward(value)".as_slice()),
                ("deeper", b"value: [[[bool, string], bool], bool]"),
                ("reference", b"value: [[&bool, bool], bool]"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), at, "{name}");
            }
            let exact = AnalysisLimits {
                max_work: unpack.statistics().work,
                max_entries: unpack.statistics().entries,
                ..default
            };
            assert!(lower("unpack", exact).mir().is_some());
            let bare = lower("depthBare", default);
            assert!(bare.mir().is_some());
            assert!(lower("depthGrouped", default).mir().is_some());
            let depth = AnalysisLimits {
                max_depth: bare.statistics().max_depth,
                ..default
            };
            assert!(lower("depthBare", depth).mir().is_some());
            let grouped = lower("depthGrouped", depth);
            crate::limit::assert_body_limit(&grouped);
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let report = lower("unpack", short);
                crate::limit::assert_body_limit(&report);
            }
        },
    );
}

#[test]
fn nested_tuple_pattern_sources_stage_owned_leaves() {
    use compiler::analysis::ownership::{Place, Projection};

    // Report missing ownership judgments after probing both installed load modes.
    let mut failures = Vec::new();
    probe(
        "patterns::nested_tuple_pattern_sources_stage_owned_leaves",
        r#"
            function control(incoming: [[string, bool], bool]): [[string, bool], bool] {
                let saved = incoming;
                return saved;
            }
            function named(incoming: [[string, bool], bool]): string {
                let saved = incoming;
                let [[text, _], _] = saved;
                return text;
            }
            function literal(seed: string): string {
                let [[text, _], _]: [[string, bool], bool] = [[seed,true],false];
                return text;
            }
            function remainder(incoming: [[string, string], string]): void {
                let saved = incoming;
                let [[text, _], _] = saved;
                let after = true;
            }
            function reuse(incoming: [[string, bool], bool]): void {
                let saved = incoming;
                let [[_, _], _] = saved;
                let again = saved;
            }
            function copied(incoming: [[bool, bool], bool]): bool {
                let saved = incoming;
                let [[first, _], _] = saved;
                let [[_, _], second] = ((saved));
                return second;
            }
            function partial(incoming: [[string, bool], bool]): void {
                let saved = incoming;
                let moved = saved[0][0];
                let [[_, _], _] = saved;
            }
        "#,
        |request, module, receipt| {
            let whole = |a: &Place<'_>, b: &Place<'_>| {
                a.root.node == b.root.node && a.projection.is_none() && b.projection.is_none()
            };
            let nested = |leaf: &Place<'_>, stage: &Place<'_>, inner| {
                leaf.root.node == stage.root.node
                    && matches!(leaf.projection, Some(Projection::NestedTupleElement { outer: 0, inner: i }) if i == inner)
            };
            let load = if request.test_mode { "test" } else { "normal" };
            for name in [
                "control",
                "named",
                "literal",
                "remainder",
                "reuse",
                "copied",
                "partial",
            ] {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &declaration.id, AnalysisLimits::default())
                        .unwrap();
                if matches!(name, "reuse" | "partial")
                    && report.completion() == Completion::SourceRejected
                {
                    assert!(report.mir().is_none());
                    let [finding] = report.findings() else {
                        panic!("{name}: expected one use-after-move finding");
                    };
                    assert_eq!(finding.code(), "R0200");
                    let function = &module.syntax.nodes[declaration.node].span;
                    let reads: Vec<_> = function
                        .bytes()
                        .windows(b"= saved".len())
                        .enumerate()
                        .filter_map(|(i, text)| {
                            (text == b"= saved").then_some(function.start() + i + 2)
                        })
                        .collect();
                    assert_eq!(
                        reads,
                        [finding.related.span.start(), finding.site.span.start()]
                    );
                    assert_eq!(finding.site.span.bytes(), b"saved");
                    assert_eq!(
                        finding.related.span.bytes(),
                        if name == "reuse" {
                            &b"saved"[..]
                        } else {
                            b"saved[0][0]"
                        }
                    );
                    continue;
                }
                let Some(mir) = report
                    .mir()
                    .filter(|_| !matches!(name, "reuse" | "partial"))
                else {
                    failures.push(format!(
                        "{load}: {name}: M4 receipt; M5 {:?}, MIR={}, at {:?}, findings={:?}",
                        report.completion(),
                        report.mir().is_some(),
                        report.blocked_at().map(|site| site
                            .span
                            .bytes()
                            .escape_ascii()
                            .to_string()),
                        report
                            .findings()
                            .iter()
                            .map(|finding| finding.code())
                            .collect::<Vec<_>>()
                    ));
                    continue;
                };
                use Operation::{Copy, Export, Move};
                let ops = linear(mir, mir.entry);
                let mut transfers = Vec::new();
                let mut exports = Vec::new();
                for (at, op) in ops.iter().enumerate() {
                    match op {
                        Move {
                            from,
                            into: Some(into),
                        } if name != "copied" => transfers.push((at, from, into)),
                        Copy {
                            from,
                            into: Some(into),
                        } if name == "copied" => transfers.push((at, from, into)),
                        Export(place) => exports.push((at, place)),
                        Move { .. } | Copy { .. } => panic!("{load}: {name}: unexpected {op:?}"),
                        _ => {}
                    }
                }
                let drops = path_drops(mir, mir.entry);
                if name != "remainder" {
                    let [(at, output)] = exports.as_slice() else {
                        panic!("{name}: expected one export");
                    };
                    let (stage_at, _, staged) = transfers.last().unwrap();
                    assert!(stage_at < at && whole(staged, output));
                    assert!(drops.is_empty(), "{name}: {drops:?}");
                }
                let saved_from = |incoming: &Place<'_>, saved: &Place<'_>, ty: &[u8]| {
                    assert_eq!(incoming.root.span.bytes(), ty);
                    assert_eq!(saved.root.span.bytes(), b"saved");
                    assert!(incoming.projection.is_none() && saved.projection.is_none());
                };
                let pattern_stage = |saved: &Place<'_>,
                                     read: &Place<'_>,
                                     stage: &Place<'_>,
                                     leaf: &Place<'_>,
                                     text: &Place<'_>| {
                    assert!(whole(saved, read));
                    assert_ne!(stage.root.node, saved.root.node);
                    assert_eq!(stage.root.span.bytes(), b"saved");
                    assert!(stage.projection.is_none() && nested(leaf, stage, 0));
                    assert_eq!(text.root.span.bytes(), b"text");
                    assert!(text.projection.is_none());
                };
                match (name, transfers.as_slice()) {
                    ("control", [(_, incoming, saved), (_, read, _)]) => {
                        saved_from(incoming, saved, b"incoming: [[string, bool], bool]");
                        assert!(whole(saved, read));
                    }
                    (
                        "named",
                        [
                            (_, incoming, saved),
                            (_, read, stage),
                            (_, leaf, text),
                            (_, returned, _),
                        ],
                    ) => {
                        saved_from(incoming, saved, b"incoming: [[string, bool], bool]");
                        pattern_stage(saved, read, stage, leaf, text);
                        assert!(whole(text, returned));
                    }
                    ("literal", [(_, seed, initialized), (_, leaf, text), (_, returned, _)]) => {
                        assert_eq!(seed.root.span.bytes(), b"seed: string");
                        assert!(seed.projection.is_none());
                        assert_eq!(leaf.root.span.bytes(), b"[[seed,true],false]");
                        assert!(nested(leaf, initialized, 0) && nested(initialized, leaf, 0));
                        assert_eq!(text.root.span.bytes(), b"text");
                        assert!(whole(text, returned));
                    }
                    (
                        "remainder",
                        [
                            (_, incoming, saved),
                            (_, read, stage),
                            (binding, leaf, text),
                        ],
                    ) => {
                        saved_from(incoming, saved, b"incoming: [[string, string], string]");
                        pattern_stage(saved, read, stage, leaf, text);
                        let after = ops.iter().position(|op| {
                            matches!(op, Operation::Initialize(place) if place.root.span.bytes() == b"after")
                        }).unwrap();
                        let mut drop_steps = Vec::new();
                        for (at, op) in ops.iter().enumerate() {
                            match op {
                                Operation::Drop(place) => drop_steps.push((at, place)),
                                Operation::Cleanup(actions) => {
                                    drop_steps.extend(actions.iter().filter_map(|action| {
                                        match action {
                                            Obligation::Drop(place) => Some((at, place)),
                                            _ => None,
                                        }
                                    }))
                                }
                                _ => {}
                            }
                        }
                        assert!(
                            matches!(drop_steps.as_slice(), [(a, outer), (b, inner), (c, bound)]
                            if outer.root.node == stage.root.node && outer.tuple_element() == Some(1)
                            && nested(inner, stage, 1) && whole(bound, text)
                            && binding < a && a <= b && *b < after && after < *c),
                            "{drop_steps:?}"
                        );
                    }
                    (
                        "copied",
                        [
                            (_, incoming, saved),
                            (_, source, stage),
                            (_, leaf, first),
                            (_, reused, next),
                            (_, sibling, second),
                            (_, returned, _),
                        ],
                    ) => {
                        saved_from(incoming, saved, b"incoming: [[bool, bool], bool]");
                        assert!(whole(saved, source) && whole(saved, reused));
                        assert!(stage.projection.is_none() && next.projection.is_none());
                        assert_ne!(stage.root.node, next.root.node);
                        assert!(nested(leaf, stage, 0));
                        assert_eq!(first.root.span.bytes(), b"first");
                        assert!(first.projection.is_none());
                        assert!(
                            sibling.root.node == next.root.node
                                && sibling.tuple_element() == Some(1)
                        );
                        assert_eq!(second.root.span.bytes(), b"second");
                        assert!(whole(second, returned));
                    }
                    _ => panic!("{load}: {name}: unexpected transfers {transfers:?}"),
                }
            }
        },
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn named_owned_record_pattern_transfers_whole_child() {
    use compiler::analysis::DeclarationShape;
    use compiler::analysis::ownership::{Place, Projection};

    let mut failures = Vec::new();
    probe_with_typed(
        "patterns::named_owned_record_pattern_transfers_whole_child",
        r#"
            type Inner = { text: string; flag: bool; }
            type Outer = { child: Inner; sibling: Inner; rest: string; }
            function leaves(incoming: Outer): string {
                let saved = incoming;
                let Outer { child: Inner { flag: _ }, sibling: Inner { text: kept } } = saved;
                let after = true;
                return kept;
            }
            function moved(incoming: Outer): string {
                let saved = incoming;
                let Outer { child: whole, sibling: Inner { text: kept } } = saved;
                let after = true;
                return kept;
            }
            function rejected(incoming: Outer): string {
                let saved = incoming;
                let Outer { child: whole, sibling: Inner { text: kept } } = saved;
                return saved.rest;
            }
            function childReuse(incoming: Outer): string {
                let Outer { child: whole } = incoming;
                let gone = whole;
                return whole.text;
            }
            function borrowed(incoming: &Outer): void {
                let Outer { child: whole } = incoming;
            }
            type Boxed = { child: Inner; flag: bool; }
            function constructed(seed: Inner): void {
                let Boxed { child: whole } = Boxed { child: seed, flag: true };
            }
            type Custom = { text: string; }
            implement dispose(value: Custom): void {}
            type CustomOuter = { child: Custom; rest: string; }
            function custom(incoming: CustomOuter): void {
                let CustomOuter { child: whole } = incoming;
            }
            type Bits = { marked: bool; }
            type Flags = { child: Bits; sibling: Bits; ready: bool; }
            function copied(incoming: Flags): bool {
                let saved = incoming;
                let Flags { child: pair, sibling: Bits { marked } } = saved;
                let after = saved.ready;
                return pair.marked;
            }
        "#,
        |request, module, receipt, typed| {
            let mode = if request.test_mode { "test" } else { "normal" };
            let field = |record, name| {
                let facts = typed
                    .inventory()
                    .type_facts()
                    .declarations()
                    .find(|facts| {
                        facts.declaration.id.module == module.key
                            && facts.declaration.name.as_deref() == Some(record)
                    })
                    .unwrap();
                let DeclarationShape::Record { fields } = &facts.shape else {
                    panic!("expected checked record fields");
                };
                &fields
                    .iter()
                    .find(|f| f.declaration.name.as_deref() == Some(name))
                    .unwrap()
                    .declaration
                    .id
            };
            let root = |a: &Place<'_>, b: &Place<'_>| a.root.node == b.root.node;
            let whole = |a: &Place<'_>, b: &Place<'_>| {
                root(a, b) && a.projection.is_none() && b.projection.is_none()
            };
            let direct = |p: &Place<'_>, base: &Place<'_>, record, name| {
                root(p, base) && p.field().is_some_and(|f| &f.id == field(record, name))
            };
            let nested = |p: &Place<'_>, base: &Place<'_>, outer_record, inner_record, name| {
                root(p, base)
                    && matches!(p.projection, Some(Projection::NestedField { outer, inner })
                    if &outer.id == field(outer_record, "sibling") && &inner.id == field(inner_record, name))
            };
            for (name, expected) in [
                ("leaves", Completion::Checked),
                ("moved", Completion::Checked),
                ("rejected", Completion::SourceRejected),
                ("childReuse", Completion::SourceRejected),
                ("copied", Completion::Checked),
                ("borrowed", Completion::Incomplete),
                ("constructed", Completion::Incomplete),
                ("custom", Completion::Incomplete),
            ] {
                let decl = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
                let evidence = format!(
                    "{mode}: {name}: M4 receipt; M5 {:?}, MIR={}, blocker={:?}, findings={:?}",
                    report.completion(),
                    report.mir().is_some(),
                    report
                        .blocked_at()
                        .map(|site| site.span.bytes().escape_ascii().to_string()),
                    report
                        .findings()
                        .iter()
                        .map(|f| f.code())
                        .collect::<Vec<_>>()
                );
                eprintln!("{evidence}");
                if report.completion() != expected {
                    failures.push(evidence);
                    continue;
                }
                if expected == Completion::Incomplete {
                    assert!(report.mir().is_none() && report.findings().is_empty());
                    continue;
                }
                if expected == Completion::SourceRejected {
                    assert!(report.mir().is_none());
                    let [finding] = report.findings() else {
                        panic!("{mode}: {name}: expected one finding");
                    };
                    let (read, moved): (&[u8], &[u8]) = if name == "rejected" {
                        (b"saved.rest", b"saved")
                    } else {
                        (b"whole.text", b"whole")
                    };
                    assert_eq!(finding.code(), "R0200");
                    assert_eq!(finding.site.span.bytes(), read);
                    assert_eq!(finding.related.span.bytes(), moved);
                    assert!(finding.related.span.start() < finding.site.span.start());
                    continue;
                }
                let mir = report.mir().unwrap();
                if name == "leaves" {
                    continue;
                }
                let ops = linear(mir, mir.entry);
                let transfers: Vec<_> = ops
                    .iter()
                    .enumerate()
                    .filter(|(_, op)| {
                        matches!(
                            op,
                            Operation::Move { .. } | Operation::Copy { .. } | Operation::Export(_)
                        )
                    })
                    .collect();
                if name == "copied" {
                    assert!(
                        matches!(transfers.as_slice(), [
                        (_, Operation::Copy { from: incoming, into: Some(saved) }),
                        (_, Operation::Copy { from: source, into: Some(stage) }),
                        (_, Operation::Copy { from: child, into: Some(pair) }),
                        (_, Operation::Copy { from: marker, into: Some(marked) }),
                        (_, Operation::Copy { from: reused, into: Some(after) }),
                        (_, Operation::Copy { from: child_leaf, into: Some(result) }),
                        (_, Operation::Export(out)),
                    ] if incoming.root.span.bytes() == b"incoming: Flags" && whole(saved, source)
                        && saved.root.span.bytes() == b"saved" && stage.projection.is_none() && !root(saved, stage)
                        && direct(child, stage, "Flags", "child") && pair.root.span.bytes() == b"pair"
                        && pair.projection.is_none() && !root(pair, stage)
                        && nested(marker, stage, "Flags", "Bits", "marked") && marked.root.span.bytes() == b"marked"
                        && direct(reused, saved, "Flags", "ready") && after.root.span.bytes() == b"after"
                        && direct(child_leaf, pair, "Bits", "marked") && whole(result, out)),
                        "{mode}: {transfers:?}"
                    );
                    assert!(path_drops(mir, mir.entry).is_empty());
                    continue;
                }
                let [
                    (
                        _,
                        Operation::Move {
                            from: incoming,
                            into: Some(saved),
                        },
                    ),
                    (
                        _,
                        Operation::Move {
                            from: source,
                            into: Some(stage),
                        },
                    ),
                    (
                        _,
                        Operation::Move {
                            from: child,
                            into: Some(bound),
                        },
                    ),
                    (
                        bind_leaf,
                        Operation::Move {
                            from: leaf,
                            into: Some(kept),
                        },
                    ),
                    (
                        return_at,
                        Operation::Move {
                            from: returned,
                            into: Some(result),
                        },
                    ),
                    (_, Operation::Export(out)),
                ] = transfers.as_slice()
                else {
                    panic!("{mode}: {transfers:?}");
                };
                assert_eq!(incoming.root.span.bytes(), b"incoming: Outer");
                assert!(incoming.projection.is_none() && whole(saved, source));
                assert_eq!(saved.root.span.bytes(), b"saved");
                assert!(stage.projection.is_none() && !root(saved, stage));
                assert!(direct(child, stage, "Outer", "child"));
                assert_eq!(bound.root.span.bytes(), b"whole");
                assert!(bound.projection.is_none() && !root(bound, stage));
                assert!(nested(leaf, stage, "Outer", "Inner", "text"));
                assert_eq!(kept.root.span.bytes(), b"kept");
                assert!(whole(kept, returned) && whole(result, out));

                let after = ops.iter().position(|op| {
                    matches!(op, Operation::Initialize(p) if p.root.span.bytes() == b"after" && p.projection.is_none())
                }).unwrap();
                let mut early = Vec::new();
                let mut late = Vec::new();
                for (at, op) in ops.iter().enumerate() {
                    let actions = if at < after { &mut early } else { &mut late };
                    match op {
                        Operation::Cleanup(cleanup) => {
                            actions.extend(cleanup.iter().map(|action| (at, *action)))
                        }
                        Operation::Drop(place) => actions.push((at, Obligation::Drop(*place))),
                        _ => {}
                    }
                }
                assert!(
                    matches!(early.as_slice(), [
                    (at, Obligation::Drop(rest)), (_, Obligation::EndStorage(flag)),
                    (_, Obligation::EndStorage(sibling)), (_, Obligation::EndStorage(header)),
                ] if bind_leaf < at && direct(rest, stage, "Outer", "rest")
                    && nested(flag, stage, "Outer", "Inner", "flag")
                    && direct(sibling, stage, "Outer", "sibling") && whole(header, stage)),
                    "{mode}: {early:?}"
                );
                assert!(!late.iter().any(|(_, action)| matches!(action,
                    Obligation::EndStorage(p) | Obligation::Drop(p)
                        if [incoming, saved, stage, kept, result].iter().any(|owner| root(p, owner))
                )), "{mode}: {late:?}");
                late.retain(|(_, action)| match action {
                    Obligation::EndStorage(p) | Obligation::Drop(p) => root(p, bound),
                    _ => false,
                });
                assert!(
                    matches!(late.as_slice(), [
                    (at, Obligation::EndStorage(flag)), (_, Obligation::Drop(text)), (_, Obligation::EndStorage(header)),
                ] if return_at < at && direct(flag, bound, "Inner", "flag")
                    && direct(text, bound, "Inner", "text") && whole(header, bound)),
                    "{mode}: {late:?}"
                );
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(rest), Obligation::Drop(text)]
                    if direct(rest, stage, "Outer", "rest") && direct(text, bound, "Inner", "text"))
                );
            }
        },
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn whole_record_child_preflight_counts_grouped_sources() {
    probe(
        "patterns::whole_record_child_preflight_counts_grouped_sources",
        r#"
        type Inner = { text: string; }
        type Outer = { child: Inner; flag: bool; }
        function bare(input: Outer): string { let Outer { child: whole } = input; return whole.text; }
        function grouped(input: Outer): string { let Outer { child: whole } = ((((((((((input)))))))))); return whole.text; }
        function constructed(input: Outer, seed: Inner): string {
            let Outer { child: whole } = ((((((((((Outer { child: seed, flag: true })))))))))); return whole.text;
        }
    "#,
        |_, module, receipt| {
            let lower = |name: &str, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            let bare = lower("bare", default);
            assert!(bare.mir().is_some());
            assert!(lower("grouped", default).mir().is_some());
            assert_eq!(
                lower("constructed", default).completion(),
                Completion::Incomplete
            );
            let limits = AnalysisLimits {
                max_depth: bare.statistics().max_depth,
                ..default
            };
            assert!(lower("bare", limits).mir().is_some());
            for name in ["grouped", "constructed"] {
                let short = lower(name, limits);
                crate::limit::assert_body_limit(&short);
            }
        },
    );
}

#[test]
fn named_owned_tuple_pattern_transfers_whole_child() {
    use compiler::analysis::ownership::Projection;

    let mut failures = Vec::new();
    probe(
        "patterns::named_owned_tuple_pattern_transfers_whole_child",
        r#"
            function exported(input: [[bool, string], [string, bool], string, string]): [string, bool] {
                let saved = input;
                let [_, child, tail, _] = (saved);
                let after = true;
                return child;
            }
            function sourceReuse(input: [[string, bool], string]): string {
                let [child, _] = input;
                return input[1];
            }
            function childReuse(input: [[string, bool], string]): string {
                let [child, _] = input;
                let gone = child;
                return child[0];
            }
            function copied(input: [[bool, bool], bool, bool]): bool {
                let [child, _, side] = input;
                let flag = input[1];
                return child[0];
            }
            function literal(seed: string): [string, bool] {
                let [child, _]: [[string, bool], bool] = [[seed, true], false];
                return child;
            }
        "#,
        |request, module, receipt| {
            let default = AnalysisLimits::default();
            let mode = if request.test_mode { "test" } else { "normal" };
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            for (name, expected) in [
                ("exported", Completion::Checked),
                ("sourceReuse", Completion::SourceRejected),
                ("childReuse", Completion::SourceRejected),
                ("copied", Completion::Checked),
                ("literal", Completion::Incomplete),
            ] {
                let report = lower(name, default);
                if report.completion() != expected {
                    failures.push(format!(
                        "{mode}: {name}: {:?} at {:?}; expected {expected:?}",
                        report.completion(),
                        report.blocked_at().map(|site| site
                            .span
                            .bytes()
                            .escape_ascii()
                            .to_string())
                    ));
                    continue;
                }
                if expected == Completion::SourceRejected {
                    assert!(report.mir().is_none());
                    let [finding] = report.findings() else {
                        panic!("{mode}: {name}: expected one finding")
                    };
                    let (read, moved) = if name == "sourceReuse" {
                        (b"input[1]".as_slice(), b"input".as_slice())
                    } else {
                        (b"child[0]".as_slice(), b"child".as_slice())
                    };
                    assert_eq!(finding.code(), "R0200");
                    assert_eq!(finding.site.span.bytes(), read);
                    assert_eq!(finding.related.span.bytes(), moved);
                    continue;
                }
                if name == "literal" {
                    assert!(report.mir().is_none() && report.findings().is_empty());
                    continue;
                }
                let mir = report.mir().unwrap();
                let ops = linear(mir, mir.entry);
                if name == "copied" {
                    let copies: Vec<_> = ops
                        .iter()
                        .filter(|op| matches!(op, Operation::Copy { .. }))
                        .collect();
                    assert!(
                        matches!(copies.as_slice(), [
                        Operation::Copy { from: input, into: Some(stage) },
                        Operation::Copy { from: child, into: Some(bound) },
                        Operation::Copy { from: side, .. },
                        Operation::Copy { from: reused, into: Some(flag) },
                        Operation::Copy { from: leaf, .. },
                    ] if input.projection.is_none() && stage.projection.is_none()
                        && child.root.node == stage.root.node && child.tuple_element() == Some(0)
                        && side.root.node == stage.root.node && side.tuple_element() == Some(2)
                        && bound.root.span.bytes() == b"child" && bound.projection.is_none()
                        && reused.root.node == input.root.node && reused.tuple_element() == Some(1)
                        && flag.root.span.bytes() == b"flag" && leaf.root.node == bound.root.node
                        && leaf.tuple_element() == Some(0)),
                        "{mode}: {copies:?}"
                    );
                    assert!(!ops.iter().any(|op| matches!(op, Operation::Move { .. })));
                    assert!(path_drops(mir, mir.entry).is_empty());
                    continue;
                }
                let transfers: Vec<_> = ops
                    .iter()
                    .filter(|op| matches!(op, Operation::Move { .. } | Operation::Export(_)))
                    .collect();
                let [
                    Operation::Move {
                        from: input,
                        into: Some(saved),
                    },
                    Operation::Move {
                        from: source,
                        into: Some(stage),
                    },
                    Operation::Move {
                        from: child,
                        into: Some(bound),
                    },
                    Operation::Move {
                        from: element,
                        into: Some(tail),
                    },
                    Operation::Move {
                        from: giving,
                        into: Some(result),
                    },
                    Operation::Export(out),
                ] = transfers.as_slice()
                else {
                    panic!("{mode}: {transfers:?}")
                };
                assert!(
                    [
                        input, saved, source, stage, bound, tail, giving, result, out
                    ]
                    .iter()
                    .all(|place| place.projection.is_none())
                );
                assert_eq!(source.root.node, saved.root.node);
                assert_ne!(source.root.node, stage.root.node);
                assert_eq!(
                    (child.root.node, child.tuple_element()),
                    (stage.root.node, Some(1))
                );
                assert_eq!(
                    (element.root.node, element.tuple_element()),
                    (stage.root.node, Some(2))
                );
                assert_eq!(bound.root.span.bytes(), b"child");
                assert_eq!(tail.root.span.bytes(), b"tail");
                assert_eq!(bound.root.node, giving.root.node);
                assert_eq!(result.root.node, out.root.node);
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(last), Obligation::Drop(first), Obligation::Drop(end)]
                    if last.root.node == stage.root.node && last.tuple_element() == Some(3)
                    && first.root.node == stage.root.node && matches!(first.projection, Some(Projection::NestedTupleElement { outer: 0, inner: 1 }))
                    && end.root.node == tail.root.node && end.projection.is_none())
                );
                let after = ops.iter().position(|op| matches!(op, Operation::Initialize(place) if place.root.span.bytes() == b"after")).unwrap();
                let last_bind = ops.iter().position(|op| matches!(op, Operation::Move { into: Some(dest), .. } if dest.root.node == tail.root.node)).unwrap();
                for (i, op) in ops.iter().enumerate() {
                    if let Operation::Cleanup(actions) = op {
                        for action in actions {
                            if let Obligation::Drop(place) = action {
                                if place.root.node == stage.root.node {
                                    assert!(last_bind < i && i < after);
                                }
                                if place.root.node == tail.root.node {
                                    assert!(after < i);
                                }
                            }
                        }
                    }
                }
                let exact = AnalysisLimits {
                    max_work: report.statistics().work,
                    max_entries: report.statistics().entries,
                    max_depth: report.statistics().max_depth,
                };
                assert!(lower(name, exact).mir().is_some());
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
                    let limited = lower(name, short);
                    crate::limit::assert_body_limit(&limited);
                }
            }
        },
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn grouped_tuple_pattern_literal_stages_only_on_reached_paths() {
    use compiler::analysis::ownership::{Place, Projection};

    probe(
        "patterns::grouped_tuple_pattern_literal_stages_only_on_reached_paths",
        r#"
            function grouped(head: string, middle: string, picked: string, tail: string): string {
                let [[_, first], _, [second, _]]: [[bool, string], string, [string, string]] =
                    (([([true, (head)]), middle, ([(picked), tail])]));
                return second;
            }
            function bareInner(seed: string): string {
                let [[text, _], _]: [[string, bool], bool] = [[seed, true], false];
                return text;
            }
            function groupedInner(seed: string): string {
                let [[text, _], _]: [[string, bool], bool] = [((((([seed, true]))))), false];
                return text;
            }
            function skipped(seed: string): string {
                if (false) {
                    let [[_, _], _]: [[string, bool], bool] = [[seed, true], false];
                }
                return seed;
            }
            function repeated(seed: string): string {
                let [[text, _], _]: [[string, string], bool] = [[seed, seed], false];
                return text;
            }
            function interrupted(seed: string, gate: Option<bool>): Option<bool> {
                let [[_, bit], _]: [[string, bool], bool] = [[seed, gate?], false];
                return Option.Some(bit);
            }
            function namedChild(child: [string, bool]): string {
                let [[text, _], _]: [[string, bool], bool] = [child, false];
                return text;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &declaration.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            let report = lower("grouped", default);
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "grouped: {:?} at {:?}",
                    report.completion(),
                    report.blocked_at()
                )
            });
            let ops = linear(mir, mir.entry);
            let moves: Vec<_> = ops
                .iter()
                .filter_map(|op| match op {
                    Operation::Move {
                        from,
                        into: Some(into),
                    } => Some((from, into)),
                    _ => None,
                })
                .collect();
            let [
                (head, left),
                (middle, sibling),
                (picked, right),
                (tail, last),
                (first_leaf, first),
                (second_leaf, second),
                (returned, output),
            ] = moves.as_slice()
            else {
                panic!("unexpected literal transfers {moves:?}");
            };
            let key = |place: &Place<'_>| {
                let route = match place.projection {
                    None => None,
                    Some(Projection::TupleElement(outer)) => Some((outer, None)),
                    Some(Projection::NestedTupleElement { outer, inner }) => {
                        Some((outer, Some(inner)))
                    }
                    _ => panic!("unexpected tuple projection: {place:?}"),
                };
                (place.root.node, route)
            };
            let root = left.root.node;
            assert_eq!(
                left.root.span.bytes(),
                b"(([([true, (head)]), middle, ([(picked), tail])]))"
            );
            let left_route = (root, Some((0, Some(1))));
            let right_route = (root, Some((2, Some(0))));
            assert_eq!(head.root.span.bytes(), b"head: string");
            assert_eq!(middle.root.span.bytes(), b"middle: string");
            assert_eq!(picked.root.span.bytes(), b"picked: string");
            assert_eq!(tail.root.span.bytes(), b"tail: string");
            assert_eq!(key(left), left_route);
            assert_eq!(key(sibling), (root, Some((1, None))));
            assert_eq!(key(right), right_route);
            assert_eq!(key(last), (root, Some((2, Some(1)))));
            assert_eq!(key(first_leaf), left_route);
            assert_eq!(key(second_leaf), right_route);
            assert_eq!(first.root.span.bytes(), b"first");
            assert_eq!(second.root.span.bytes(), b"second");
            assert_eq!(key(second), key(returned));
            assert!(
                ops.iter()
                    .any(|op| matches!(op, Operation::Export(place) if key(place) == key(output)))
            );
            let drops = path_drops(mir, mir.entry);
            assert!(
                matches!(drops.as_slice(), [Obligation::Drop(a), Obligation::Drop(b), Obligation::Drop(c)]
                if key(a) == key(last) && key(b) == key(sibling) && key(c) == key(first))
            );

            let exact = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                max_depth: report.statistics().max_depth,
            };
            assert!(lower("grouped", exact).mir().is_some());
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
                let bounded = lower("grouped", short);
                crate::limit::assert_body_limit(&bounded);
            }

            let bare = lower("bareInner", default);
            assert!(bare.mir().is_some() && lower("groupedInner", default).mir().is_some());
            let depth = AnalysisLimits {
                max_depth: bare.statistics().max_depth,
                ..default
            };
            assert!(lower("bareInner", depth).mir().is_some());
            let limited = lower("groupedInner", depth);
            crate::limit::assert_body_limit(&limited);

            let skipped = lower("skipped", default);
            let mir = skipped.mir().expect("a dead literal does not move seed");
            let values: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. } | Operation::Initialize(_) | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(values.as_slice(), [Operation::Move { from, into: Some(result) }, Operation::Export(out)]
                if from.root.span.bytes() == b"seed: string" && from.projection.is_none() && key(result) == key(out))
            );
            assert!(path_drops(mir, mir.entry).is_empty());

            let repeated = lower("repeated", default);
            assert_eq!(repeated.completion(), Completion::SourceRejected);
            assert!(repeated.mir().is_none());
            let [finding] = repeated.findings() else {
                panic!("expected one moved literal input finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"seed");
            assert_eq!(finding.related.span.bytes(), b"seed");
            assert!(finding.related.span.start() < finding.site.span.start());
            let interrupted = lower("interrupted", default);
            assert_eq!(interrupted.completion(), Completion::Incomplete);
            assert!(interrupted.mir().is_none() && interrupted.findings().is_empty());
            let child = lower("namedChild", default);
            assert_eq!(child.completion(), Completion::Incomplete);
            assert!(child.mir().is_none());
            assert_eq!(child.blocked_at().unwrap().span.bytes(), b"child");
        },
    );
}

#[test]
fn unannotated_nested_tuple_pattern_type_boundary() {
    probe_report(
        "patterns::unannotated_nested_tuple_pattern_type_boundary",
        r#"
            function unannotated(seed: string): string {
                let [[text, _], _] = [[seed, true], false];
                return text;
            }
        "#,
        |_, _, typed| assert!(typed.receipt().is_none()),
    );
}

#[test]
fn shared_nested_tuple_patterns_keep_exact_leaf_routes() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, SharedView},
        Projection,
    };

    probe(
        "patterns::shared_nested_tuple_patterns_keep_exact_leaf_routes",
        r#"
            function view(value: &[[bool, string], bool]): &string {
                let ([([flag, (text)]), _]) = value;
                let _ = flag;
                return text;
            }
            function mixed(value: &[bool, [bool, bool]]): bool {
                let [outer, [_, inner]] = value;
                let _ = outer;
                return inner;
            }
            function aliasEnds(value: &[[bool, string], bool], fallback: &string): &string {
                let mut out = fallback;
                { let alias = value; let [[_, text], _] = alias; out = text; }
                return out;
            }
            function ignores(value: &[[string, string], bool]): bool {
                let [[_, _], bit] = value;
                return bit;
            }
            function ownedNested(value: [[bool, string], bool]): bool {
                let [[bit, _], _] = value;
                return bit;
            }
            function nestedName(value: &[[bool, bool], bool]): bool {
                let [pair, bit] = value;
                let _ = pair;
                return bit;
            }
            function deeper(value: &[[[bool, bool], bool], bool]): bool {
                let [[[bit, _], _], _] = value;
                return bit;
            }
            function mixedReference(value: &[&bool, [bool, bool]]): &bool {
                let [saved, [bit, _]] = value;
                let _ = bit;
                return saved;
            }
            function nestedReference(value: &[[&bool, bool], bool]): &bool {
                let [[saved, _], _] = value;
                return saved;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let check = |name| lower(name, AnalysisLimits::default());
            for name in ["view", "mixed", "aliasEnds", "ignores"] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                assert!(path_drops(mir, mir.entry).is_empty());
                let ops = linear(mir, mir.entry);
                assert!(
                    !ops.iter()
                        .any(|op| matches!(op, Operation::Move { .. } | Operation::Dispose { .. }))
                );
                match name {
                    "view" => {
                        let route: Vec<_> = ops
                            .into_iter()
                            .filter(|op| {
                                matches!(
                                    op,
                                    Operation::Copy { .. }
                                        | Operation::BindBorrowedMember { .. }
                                        | Operation::Export(_)
                                )
                            })
                            .collect();
                        assert!(
                            matches!(route.as_slice(), [
                            Operation::Copy { from, into: Some(staged) },
                            Operation::BindBorrowedMember { descriptor: first,
                                projection: Projection::NestedTupleElement { outer: 0, inner: 0 }, into: flag, binding: CopyScalar },
                            Operation::BindBorrowedMember { descriptor: second,
                                projection: Projection::NestedTupleElement { outer: 0, inner: 1 }, into: text, binding: SharedView },
                            Operation::Copy { from: bit, .. },
                            Operation::Copy { from: returned, into: Some(result) },
                            Operation::Export(output),
                        ] if from.root.span.bytes() == b"value: &[[bool, string], bool]"
                            && staged.root.node == first.root.node && staged.root.node == second.root.node
                            && flag.root.span.bytes() == b"flag" && text.root.span.bytes() == b"text"
                            && flag.root.node == bit.root.node && text.root.node == returned.root.node
                            && output.root.node == result.root.node),
                            "{route:?}"
                        );
                    }
                    "mixed" | "ignores" => {
                        let routes: Vec<_> = ops
                            .into_iter()
                            .filter(|op| matches!(op, Operation::BindBorrowedMember { .. }))
                            .collect();
                        if name == "mixed" {
                            assert!(
                                matches!(
                                    routes.as_slice(),
                                    [
                                        Operation::BindBorrowedMember {
                                            projection: Projection::TupleElement(0),
                                            binding: CopyScalar,
                                            ..
                                        },
                                        Operation::BindBorrowedMember {
                                            projection: Projection::NestedTupleElement {
                                                outer: 1,
                                                inner: 1
                                            },
                                            binding: CopyScalar,
                                            ..
                                        },
                                    ]
                                ),
                                "{routes:?}"
                            );
                        } else {
                            assert!(
                                matches!(
                                    routes.as_slice(),
                                    [Operation::BindBorrowedMember {
                                        projection: Projection::TupleElement(1),
                                        binding: CopyScalar,
                                        ..
                                    }]
                                ),
                                "{routes:?}"
                            );
                        }
                    }
                    "aliasEnds" => {
                        let ended = ops.iter().position(|op| matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action|
                            matches!(action, Obligation::EndStorage(place) if place.root.span.bytes() == b"alias")))).unwrap();
                        let transfer: Vec<_> = ops[..ended]
                            .iter()
                            .copied()
                            .filter(|op| match op {
                                Operation::BindBorrowedMember { .. }
                                | Operation::Install { .. } => true,
                                Operation::Copy { from, .. } => from.root.span.bytes() == b"text",
                                _ => false,
                            })
                            .collect();
                        assert!(
                            matches!(transfer.as_slice(), [
                            Operation::BindBorrowedMember {
                                projection: Projection::NestedTupleElement { outer: 0, inner: 1 },
                                into: text, binding: SharedView, ..
                            },
                            Operation::Copy { from, into: Some(staged) },
                            Operation::Install { target, incoming },
                        ] if text.root.node == from.root.node && staged.root.node == incoming.root.node
                            && target.root.span.bytes() == b"out"),
                            "{transfer:?}"
                        );
                        assert!(
                            matches!(ops[ended + 1..].iter().copied().filter(|op| matches!(op, Operation::Copy { .. } | Operation::Export(_))).collect::<Vec<_>>().as_slice(), [
                            Operation::Copy { from, into: Some(result) }, Operation::Export(output)
                        ] if from.root.span.bytes() == b"out" && result.root.node == output.root.node)
                        );
                    }
                    _ => unreachable!(),
                }
            }
            let owned = check("ownedNested");
            let mir = owned
                .mir()
                .expect("the previous storage-boundary witness now reaches its pattern");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                if text.root.span.bytes() == b"value" && matches!(text.projection, Some(Projection::NestedTupleElement { outer: 0, inner: 1 })))
            );
            let view = check("view");
            let limits = AnalysisLimits {
                max_work: view.statistics().work,
                max_entries: view.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("view", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = lower("view", short);
                crate::limit::assert_body_limit(&limited);
            }
            for (name, site) in [
                ("nestedName", "pair"),
                ("deeper", "[[[bit, _], _], _]"),
                ("nestedReference", "[[saved, _], _]"),
                ("mixedReference", "[saved, [bit, _]]"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    site.as_bytes(),
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn shared_tuple_binding_copies_scalars_and_borrows_string_members() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, SharedView},
        Projection,
    };

    probe(
        "patterns::shared_tuple_binding_copies_scalars_and_borrows_string_members",
        r#"
            function view(value: &[bool, string, string]): &string {
                let ([flag, (text), _]) = value;
                let _ = flag;
                return text;
            }
            function ignores(value: &[bool, string]): void { let [_, _] = value; }
            function pass(value: [bool, string]): [bool, string] { return value; }
            function temporary(value: [bool, string]): &string {
                let [_, text] = &pass(value);
                return text;
            }
            function copyOut(value: [bool, string]): bool {
                let [flag, _] = &value;
                let moved = value;
                return flag;
            }
            function escapes(value: [bool, string]): &string {
                let [_, text] = &value;
                return text;
            }
            function premature(value: [bool, string]): void {
                let whole = &value;
                let [_, text] = whole;
                let moved = value;
                let _ = text;
            }
            function partial(value: [bool, string]): bool {
                let moved = value[1];
                let [flag, _] = &value;
                return flag;
            }
            function lastUse(value: [bool, string]): void {
                let [_, text] = &value;
                let _ = text;
                let moved = value;
            }
            function expires(seed: &string, flag: bool): &string {
                let mut out = seed;
                if (flag) { let pair: [bool, string] = [true, "local"]; let [_, view] = &pair; out = view; }
                return out;
            }
            function exclusive(value: &mut [bool, bool]): bool {
                let [bit, _] = value;
                return bit;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let check = |name| lower(name, AnalysisLimits::default());
            let view = check("view");
            let mir = view
                .mir()
                .unwrap_or_else(|| panic!("view: {:?}", view.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let view_ops = linear(mir, mir.entry);
            assert!(
                !view_ops
                    .iter()
                    .any(|op| matches!(op, Operation::Move { .. } | Operation::Dispose { .. }))
            );
            let route: Vec<_> = view_ops
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: incoming, into: Some(staged) },
                Operation::BindBorrowedMember { descriptor: first, projection: Projection::TupleElement(0), into: flag, binding: CopyScalar },
                Operation::BindBorrowedMember { descriptor: second, projection: Projection::TupleElement(1), into: text, binding: SharedView },
                Operation::Copy { from: bit, .. },
                Operation::Copy { from: view, into: Some(result) },
                Operation::Export(output),
            ] if incoming.root.span.bytes() == b"value: &[bool, string, string]"
                && staged.root.node == first.root.node && staged.root.node == second.root.node
                && flag.root.span.bytes() == b"flag" && flag.root.node == bit.root.node
                && text.root.span.bytes() == b"text" && text.root.node == view.root.node
                && result.root.node == output.root.node));
            let limits = AnalysisLimits {
                max_work: view.statistics().work,
                max_entries: view.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("view", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = lower("view", short);
                crate::limit::assert_body_limit(&limited);
            }
            let ignored = check("ignores");
            let mir = ignored.mir().expect("wildcards observe no member");
            let significant: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::Dispose { .. }
                            | Operation::BindBorrowedMember { .. }
                    )
                })
                .collect();
            assert!(
                matches!(significant.as_slice(), [Operation::Copy { from, into: Some(_) }]
                if from.root.span.bytes() == b"value: &[bool, string]")
            );
            assert!(path_drops(mir, mir.entry).is_empty());
            for name in ["copyOut", "lastUse"] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                    if place.root.span.bytes() == b"moved" && place.tuple_element() == Some(1))
                );
            }
            for (name, code, site, related) in [
                ("escapes", "R0202", "return text;", "value: [bool, string]"),
                ("premature", "R0203", "value", "&value"),
                ("partial", "R0200", "&value", "value[1]"),
                (
                    "expires",
                    "R0203",
                    "{ let pair: [bool, string] = [true, \"local\"]; let [_, view] = &pair; out = view; }",
                    "&pair",
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected a single finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for (name, pattern) in [("exclusive", "[bit, _]"), ("temporary", "pass(value)")] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    pattern.as_bytes(),
                    "{name}"
                );
            }
        },
    );
}
