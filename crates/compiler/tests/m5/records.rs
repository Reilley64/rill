use super::*;

fn canonical_record_field<'g>(
    typed: &TypedReport<'g>,
    module: &loader::ModuleKey,
    record: &str,
    name: &str,
) -> &'g loader::DeclarationId {
    use compiler::analysis::DeclarationShape;
    let facts = typed
        .inventory()
        .type_facts()
        .declarations()
        .find(|facts| {
            facts.declaration.id.module == *module
                && facts.declaration.name.as_deref() == Some(record)
        })
        .unwrap();
    let DeclarationShape::Record { fields } = &facts.shape else {
        panic!("expected checked record fields")
    };
    &fields
        .iter()
        .find(|member| member.declaration.name.as_deref() == Some(name))
        .unwrap()
        .declaration
        .id
}

#[test]
fn nested_owned_records_clean_initialized_leaves_on_exit() {
    use compiler::analysis::ownership::Projection;

    probe_with_typed(
        "records::nested_owned_records_clean_initialized_leaves_on_exit",
        r#"
            import std::core::dispose as release;
            type Inner = { text: string; }
            type Outer = { child: Inner; sibling: string; }
            function cleanup(value: Outer): void {}
            function exits(flag: bool, value: Outer): void { if (flag) { return; } }
            function wholeMove(value: Outer): Outer { return value; }
            function borrow(value: Outer): void { let held = &value; }
            function construct(child: Inner): void { let value = Outer { child: child, sibling: "sibling" }; }
            type Pair = { front: string; marker: i32; back: string; }
            type Stacked = { first: Pair; middle: string; last: Pair; }
            function ordered(flag: bool, value: Stacked): void {
                if (flag) { let arm = "early"; return; }
            }
            type GuardRoot = { child: Inner; lock: LockGuard<i32>; }
            function protectedSibling(value: GuardRoot): void {}
            type CustomLeaf = { text: string; }
            implement release(value: CustomLeaf): void {}
            type CustomChild = { child: CustomLeaf; sibling: string; }
            function customChild(value: CustomChild): void {}
            type CustomRoot = { child: Inner; sibling: string; }
            implement release(value: CustomRoot): void {}
            function customRoot(value: CustomRoot): void {}
            type Deeper = { child: Outer; }
            function deeper(value: Deeper): void {}
            type RefLeaf = { view: &bool; }
            type RefRoot = { child: RefLeaf; sibling: string; }
            function referenceLeaf(value: RefRoot): void {}
        "#,
        |_, module, receipt, typed| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap()
            };
            let field = |record, name| canonical_record_field(typed, &module.key, record, name);
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let default = AnalysisLimits::default();
            let check_drops = |mir: &Mir<'_>, entry| {
                assert!(
                    matches!(path_drops(mir, entry).as_slice(), [Obligation::Drop(sibling), Obligation::Drop(text)]
                    if sibling.root.span.bytes() == b"value: Outer" && text.root.node == sibling.root.node
                        && matches!(sibling.projection, Some(Projection::Field(decl)) if &decl.id == field("Outer", "sibling"))
                        && matches!(text.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", "child") && &inner.id == field("Inner", "text")))
                );
            };
            let cleanup = lower("cleanup", default);
            let mir = cleanup
                .mir()
                .unwrap_or_else(|| panic!("nested cleanup: {:?}", cleanup.completion()));
            check_drops(mir, mir.entry);
            let exits = lower("exits", default);
            let mir = exits
                .mir()
                .expect("the explicit return and fallthrough both clean the record");
            let (yes, no) = branch_targets(mir);
            check_drops(mir, yes);
            check_drops(mir, no);
            let ordered = lower("ordered", default);
            let mir = ordered
                .mir()
                .expect("each initialized nested leaf has its own exact cleanup place");
            let (yes, no) = branch_targets(mir);
            let expected = [
                ("last", Some("back")),
                ("last", Some("front")),
                ("middle", None),
                ("first", Some("back")),
                ("first", Some("front")),
            ];
            for (entry, local) in [(yes, true), (no, false)] {
                let drops = path_drops(mir, entry);
                if local {
                    assert!(matches!(drops.first(), Some(Obligation::Drop(arm))
                        if arm.root.span.bytes() == b"arm" && arm.projection.is_none()));
                }
                let fields = &drops[usize::from(local)..];
                assert_eq!(fields.len(), expected.len());
                for (drop, (outer, inner)) in fields.iter().zip(expected) {
                    let Obligation::Drop(place) = drop else {
                        panic!("expected a string leaf drop")
                    };
                    assert_eq!(place.root.span.bytes(), b"value: Stacked");
                    match (place.projection, inner) {
                        (Some(Projection::Field(member)), None) => {
                            assert_eq!(&member.id, field("Stacked", outer))
                        }
                        (
                            Some(Projection::NestedField {
                                outer: parent,
                                inner: member,
                            }),
                            Some(inner),
                        ) => {
                            assert_eq!(&parent.id, field("Stacked", outer));
                            assert_eq!(&member.id, field("Pair", inner));
                        }
                        _ => panic!("expected the canonical field path"),
                    }
                }
            }
            let whole = lower("wholeMove", default);
            let mir = whole
                .mir()
                .expect("an untouched incoming record can transfer directly to the return slot");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::Export(_)))
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Move { from, into: Some(staged) }, Operation::Export(output)]
                if from.root.span.bytes() == b"value: Outer" && from.projection.is_none() && staged.projection.is_none()
                    && staged.root.node == output.root.node && output.root.span.bytes() == b"value")
            );
            assert!(path_drops(mir, mir.entry).is_empty());
            for (name, at) in [
                ("borrow", b"value".as_slice()),
                ("construct", b"value"),
                ("protectedSibling", b"value: GuardRoot"),
                ("customChild", b"value: CustomChild"),
                ("customRoot", b"value: CustomRoot"),
                ("deeper", b"value: Deeper"),
                ("referenceLeaf", b"value: RefRoot"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                let blocker = report.blocked_at().unwrap();
                let body = &module.syntax.nodes[declaration(name).node].span;
                assert_eq!(blocker.module, &module.key);
                assert!(body.start() <= blocker.span.start() && blocker.span.end() <= body.end());
                assert_eq!(blocker.span.bytes(), at, "{name}");
            }
            let exact = AnalysisLimits {
                max_work: cleanup.statistics().work,
                max_entries: cleanup.statistics().entries,
                ..default
            };
            assert!(lower("cleanup", exact).mir().is_some());
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
                let limited = lower("cleanup", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn moving_a_nested_record_field_leaves_its_sibling_initialized() {
    use compiler::analysis::ownership::Projection;

    probe_with_typed(
        "records::moving_a_nested_record_field_leaves_its_sibling_initialized",
        r#"
            type Inner = { text: string; }
            type Outer = { child: Inner; sibling: string; }
            function branch(flag: bool, value: Outer): string {
                if (flag) { let taken = value.child; }
                return value.sibling;
            }
            function backedge(flag: bool, value: Outer): void {
                while (flag) { let taken = value.child; continue; }
            }
            function reread(flag: bool, value: Outer): Inner {
                if (flag) { let taken = value.child; }
                return value.child;
            }
            type Pair = { front: string; marker: i32; back: string; }
            type Stacked = { first: Pair; middle: string; last: Pair; }
            function takeLast(value: Stacked): void { let taken = (value.last); }
            type Scalars = { flag: bool; marker: i32; }
            type CopyChild = { child: Scalars; sibling: string; }
            function copyChild(value: CopyChild): bool {
                let first = value.child;
                let again = value.child;
                return again.flag;
            }
            type BitOuter = { child: Inner; bit: bool; }
            function copyBit(value: BitOuter): bool {
                let taken = value.child;
                let first = value.bit;
                let again = value.bit;
                return again;
            }
            function borrowField(value: Outer): void { let held = &value.child; }
            function nestedLeaf(value: Outer): string { return value.child.text; }
        "#,
        |_, module, receipt, typed| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap()
            };
            let field = |record, name| canonical_record_field(typed, &module.key, record, name);
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let default = AnalysisLimits::default();
            let branch = lower("branch", default);
            let mir = branch
                .mir()
                .unwrap_or_else(|| panic!("nested child move: {:?}", branch.completion()));
            let (yes, no) = branch_targets(mir);
            for (entry, moved) in [(yes, true), (no, false)] {
                let ops = linear(mir, entry);
                let moves: Vec<_> = ops
                    .iter()
                    .copied()
                    .filter(|op| matches!(op, Operation::Move { .. } | Operation::Export(_)))
                    .collect();
                let (child, remaining) = if moved {
                    (Some(moves[0]), &moves[1..])
                } else {
                    (None, moves.as_slice())
                };
                let [
                    Operation::Move {
                        from: sibling,
                        into: Some(staged),
                    },
                    Operation::Export(output),
                ] = remaining
                else {
                    panic!("expected the sibling move and export")
                };
                assert_eq!(sibling.root.span.bytes(), b"value: Outer");
                assert_eq!(&sibling.field().unwrap().id, field("Outer", "sibling"));
                assert_eq!(staged.root.node, output.root.node);
                let drops = path_drops(mir, entry);
                let [Obligation::Drop(text)] = drops.as_slice() else {
                    panic!("drop only the child string")
                };
                if let Some(child) = child {
                    assert!(matches!(child, Operation::Move { from, into: Some(taken) }
                        if from.root.node == sibling.root.node && from.field().is_some_and(|member| &member.id == field("Outer", "child"))
                            && taken.root.span.bytes() == b"taken" && taken.root.node == text.root.node
                            && text.field().is_some_and(|member| &member.id == field("Inner", "text"))));
                } else {
                    assert_eq!(text.root.node, sibling.root.node);
                    assert!(
                        matches!(text.projection, Some(Projection::NestedField { outer, inner })
                        if &outer.id == field("Outer", "child") && &inner.id == field("Inner", "text"))
                    );
                }
                let export = ops
                    .iter()
                    .position(|op| matches!(op, Operation::Export(_)))
                    .unwrap();
                let drop = ops.iter().position(|op| matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action, Obligation::Drop(_))))).unwrap();
                assert_eq!(drop < export, moved);
            }
            let grouped = lower("takeLast", default);
            let mir = grouped
                .mir()
                .expect("moving a grouped subtree transfers all its initialized fields");
            let transfer: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::Cleanup(_)))
                .collect();
            let [
                Operation::Move {
                    from: source,
                    into: Some(taken),
                },
                Operation::Cleanup(taken_cleanup),
                Operation::Cleanup(source_cleanup),
            ] = transfer.as_slice()
            else {
                panic!("expected one child transfer and the two cleanup boundaries")
            };
            assert_eq!(source.root.span.bytes(), b"value: Stacked");
            assert_eq!(&source.field().unwrap().id, field("Stacked", "last"));
            assert_eq!(taken.root.span.bytes(), b"taken");
            assert!(taken.projection.is_none());
            assert!(
                matches!(taken_cleanup.as_slice(), [Obligation::Drop(_), Obligation::EndStorage(marker), Obligation::Drop(_), Obligation::EndStorage(header)]
                if marker.root.node == taken.root.node && marker.field().is_some_and(|member| &member.id == field("Pair", "marker"))
                    && header.root.node == taken.root.node && header.projection.is_none())
            );
            assert!(
                matches!(source_cleanup.as_slice(), [Obligation::Drop(_), Obligation::Drop(_), Obligation::EndStorage(marker), Obligation::Drop(_), Obligation::EndStorage(child), Obligation::EndStorage(root)]
                if [marker, child, root].iter().all(|place| place.root.node == source.root.node)
                    && matches!(marker.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Stacked", "first") && &inner.id == field("Pair", "marker"))
                    && child.field().is_some_and(|member| &member.id == field("Stacked", "first")) && root.projection.is_none())
            );
            let drops = path_drops(mir, mir.entry);
            let expected = [
                (b"taken".as_slice(), "Pair", "back", None),
                (b"taken".as_slice(), "Pair", "front", None),
                (b"value: Stacked".as_slice(), "Stacked", "middle", None),
                (
                    b"value: Stacked".as_slice(),
                    "Stacked",
                    "first",
                    Some("back"),
                ),
                (
                    b"value: Stacked".as_slice(),
                    "Stacked",
                    "first",
                    Some("front"),
                ),
            ];
            assert_eq!(drops.len(), expected.len());
            for (drop, (root, record, outer, inner)) in drops.iter().zip(expected) {
                let Obligation::Drop(place) = drop else {
                    panic!("expected an owned leaf drop")
                };
                assert_eq!(place.root.span.bytes(), root);
                match (place.projection, inner) {
                    (Some(Projection::Field(member)), None) => {
                        assert_eq!(&member.id, field(record, outer))
                    }
                    (
                        Some(Projection::NestedField {
                            outer: parent,
                            inner: member,
                        }),
                        Some(inner),
                    ) => {
                        assert_eq!(&parent.id, field(record, outer));
                        assert_eq!(&member.id, field("Pair", inner));
                    }
                    _ => panic!("expected the exact transferred or remaining field"),
                }
            }
            let copied = lower("copyChild", default);
            let mir = copied
                .mir()
                .expect("a copyable child stays initialized after extraction");
            let copies: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter_map(|op| match op {
                    Operation::Copy {
                        from,
                        into: Some(dest),
                    } if from.root.span.bytes() == b"value: CopyChild" => Some((from, dest)),
                    _ => None,
                })
                .collect();
            assert!(matches!(copies.as_slice(), [(from, first), (again, second)]
                if from.root.node == again.root.node && first.root.span.bytes() == b"first" && second.root.span.bytes() == b"again"
                    && [from, again].iter().all(|place| place.field().is_some_and(|member| &member.id == field("CopyChild", "child")))));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(remaining)]
                if remaining.root.span.bytes() == b"value: CopyChild" && remaining.field().is_some_and(|member| &member.id == field("CopyChild", "sibling")))
            );
            let direct_bit = lower("copyBit", default);
            let mir = direct_bit
                .mir()
                .expect("a moved child does not affect copies of the scalar sibling");
            let copies: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter_map(|op| match op {
                    Operation::Copy {
                        from,
                        into: Some(dest),
                    } if from.root.span.bytes() == b"value: BitOuter" => Some((from, dest)),
                    _ => None,
                })
                .collect();
            assert!(matches!(copies.as_slice(), [(from, first), (again, second)]
                if from.root.node == again.root.node && first.root.span.bytes() == b"first" && second.root.span.bytes() == b"again"
                    && [from, again].iter().all(|place| place.field().is_some_and(|member| &member.id == field("BitOuter", "bit")))));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                if text.root.span.bytes() == b"taken" && text.field().is_some_and(|member| &member.id == field("Inner", "text")))
            );
            for name in ["reread", "backedge"] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected one moved field finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), b"value.child");
                assert_eq!(finding.related.span.bytes(), b"value.child");
                let body = &module.syntax.nodes[declaration(name).node].span;
                for site in [&finding.site, &finding.related] {
                    assert_eq!(site.module, &module.key);
                    assert!(body.start() <= site.span.start() && site.span.end() <= body.end());
                }
                if name == "backedge" {
                    assert_eq!(finding.site.node, finding.related.node);
                } else {
                    assert!(finding.related.span.start() < finding.site.span.start());
                }
            }
            for (name, site) in [
                ("borrowField", b"value.child".as_slice()),
                ("nestedLeaf", b"value.child.text"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete);
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site);
            }
            let exact = AnalysisLimits {
                max_work: branch.statistics().work,
                max_entries: branch.statistics().entries,
                ..default
            };
            assert!(lower("branch", exact).mir().is_some());
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
                let limited = lower("branch", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn whole_nested_record_transfers_rehome_cleanup_and_require_initialized_children() {
    use compiler::analysis::ownership::Projection;

    probe_with_typed(
        "records::whole_nested_record_transfers_rehome_cleanup_and_require_initialized_children",
        r#"
            type Inner = { text: string; }
            type Outer = { child: Inner; sibling: string; }
            function localExit(value: Outer): void { let saved = (value); }
            function give(value: Outer): Outer {
                let saved = value;
                return (saved);
            }
            function afterWhole(value: Outer): string {
                let saved = value;
                let taken = saved.child;
                return saved.sibling;
            }
            function partial(flag: bool, value: Outer): Outer {
                if (flag) { let taken = value.child; }
                return value;
            }
            function partialLocal(value: Outer): void {
                let saved = value;
                let taken = saved.child;
                let again = saved;
            }
            function movedAgain(value: Outer): void {
                let saved = value;
                let again = value;
            }
            function borrowedLocal(value: Outer): void { let saved = value; let borrowed = &saved; }
            function replaceLocal(value: Outer, replacement: Outer): void {
                let mut saved = value;
                saved = replacement;
            }
            function consume(value: Outer): bool { return true; }
            function argument(value: Outer): bool { return consume(value); }
            function result(value: Outer): Outer { return give(value); }
            type Bits = { ok: bool; }
            type Flags = { child: Bits; ready: bool; }
            function copyWhole(value: Flags): Flags {
                let first = value;
                let second = value;
                return (second);
            }
        "#,
        |_, module, receipt, typed| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap()
            };
            let field = |record, name| canonical_record_field(typed, &module.key, record, name);
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let default = AnalysisLimits::default();
            let local = lower("localExit", default);
            let mir = local
                .mir()
                .unwrap_or_else(|| panic!("whole nested transfer: {:?}", local.completion()));
            let moves: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Move { .. }))
                .collect();
            let [
                Operation::Move {
                    from,
                    into: Some(saved),
                },
            ] = moves.as_slice()
            else {
                panic!("expected one whole transfer")
            };
            assert_eq!(from.root.span.bytes(), b"value: Outer");
            assert_eq!(saved.root.span.bytes(), b"saved");
            assert!(from.projection.is_none() && saved.projection.is_none());
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(sibling), Obligation::Drop(text)]
                if sibling.root.node == saved.root.node && text.root.node == saved.root.node
                    && sibling.field().is_some_and(|member| &member.id == field("Outer", "sibling"))
                    && matches!(text.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", "child") && &inner.id == field("Inner", "text")))
            );
            let exported = lower("give", default);
            let mir = exported
                .mir()
                .expect("export the complete nested value without local drops");
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::Export(_)))
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Move { from: input, into: Some(saved) },
                Operation::Move { from: stored, into: Some(staged) }, Operation::Export(output),
            ] if input.root.span.bytes() == b"value: Outer" && saved.root.span.bytes() == b"saved"
                && saved.root.node == stored.root.node && staged.root.node == output.root.node && output.root.span.bytes() == b"(saved)"
                && [input, saved, stored, staged, output].iter().all(|place| place.projection.is_none())));
            let partial_transfer = lower("afterWhole", default);
            let mir = partial_transfer
                .mir()
                .expect("direct field transfers also work from the moved-to local");
            let ops = linear(mir, mir.entry);
            let moves: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::Export(_)))
                .collect();
            let [
                Operation::Move {
                    into: Some(saved), ..
                },
                Operation::Move {
                    from: child,
                    into: Some(taken),
                },
                Operation::Move {
                    from: sibling,
                    into: Some(staged),
                },
                Operation::Export(output),
            ] = moves.as_slice()
            else {
                panic!("expected root, child, sibling and export")
            };
            assert_eq!(saved.root.node, child.root.node);
            assert_eq!(saved.root.node, sibling.root.node);
            assert_eq!(&child.field().unwrap().id, field("Outer", "child"));
            assert_eq!(&sibling.field().unwrap().id, field("Outer", "sibling"));
            assert_eq!(staged.root.node, output.root.node);
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                if text.root.node == taken.root.node && text.field().is_some_and(|member| &member.id == field("Inner", "text")))
            );
            let export_at = ops
                .iter()
                .position(|op| matches!(op, Operation::Export(_)))
                .unwrap();
            let drop_at = ops.iter().position(|op| matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action, Obligation::Drop(_))))).unwrap();
            assert!(export_at < drop_at);

            let copied = lower("copyWhole", default);
            let mir = copied
                .mir()
                .expect("copyable nested roots remain initialized");
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
                Operation::Copy { from: input, into: Some(first) },
                Operation::Copy { from: same, into: Some(second) },
                Operation::Copy { from: stored, into: Some(staged) }, Operation::Export(output),
            ] if input.root.span.bytes() == b"value: Flags" && input.root.node == same.root.node
                && first.root.span.bytes() == b"first" && second.root.span.bytes() == b"second"
                && second.root.node == stored.root.node && staged.root.node == output.root.node
                && [input, first, same, second, stored, staged, output].iter().all(|place| place.projection.is_none())));
            assert!(path_drops(mir, mir.entry).is_empty());
            for (name, site, related) in [
                ("partial", b"value".as_slice(), b"value.child".as_slice()),
                ("partialLocal", b"saved", b"saved.child"),
                ("movedAgain", b"value", b"value"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected one incomplete initialization finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), related);
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            for (name, site) in [
                ("borrowedLocal", b"saved".as_slice()),
                ("replaceLocal", b"saved"),
                ("argument", b"value"),
                ("result", b"give(value)"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete);
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site);
            }
            let exact = AnalysisLimits {
                max_work: partial_transfer.statistics().work,
                max_entries: partial_transfer.statistics().entries,
                ..default
            };
            assert!(lower("afterWhole", exact).mir().is_some());
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
                let limited = lower("afterWhole", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn restoring_a_nested_record_child_repairs_the_whole_value_and_backedge() {
    use compiler::analysis::ownership::Projection;

    probe_with_typed(
        "records::restoring_a_nested_record_child_repairs_the_whole_value_and_backedge",
        r#"
            type Inner = { first: string; flag: bool; last: string; }
            type Outer = { child: Inner; sibling: string; }
            function join(seed: Outer, fresh: Inner, take: bool): Outer {
                let mut saved = seed;
                if (take) { let gone = saved.child; }
                saved.child = (fresh);
                return saved;
            }
            function loopRestore(seed: Outer, again: bool): Outer {
                let mut saved = seed;
                while (again) {
                    let taken = saved.child;
                    saved.child = taken;
                    continue;
                }
                return saved;
            }
            function skipRestore(seed: Outer, again: bool, skip: bool): void {
                let mut saved = seed;
                while (again) {
                    let taken = saved.child;
                    if (skip) { continue; }
                    saved.child = taken;
                }
            }
            function repairOneEdge(seed: Outer, repair: bool): Outer {
                let mut saved = seed;
                let taken = saved.child;
                if (repair) { saved.child = taken; }
                return saved;
            }
            function movedRoot(seed: Outer, fresh: Inner): void {
                let mut saved = seed;
                let gone = saved;
                saved.child = fresh;
            }
            function deep(seed: Outer, fresh: string): void {
                let mut saved = seed;
                saved.child.first = fresh;
            }
            function scalar(seed: Outer, fresh: string): void {
                let mut saved = seed;
                saved.sibling = fresh;
            }
            function projected(seed: Outer, fresh: Outer): void {
                let mut saved = seed;
                saved.child = fresh.child;
            }
            function construct(seed: Outer, first: string, last: string): void {
                let mut saved = seed;
                saved.child = Inner { first: first, flag: false, last: last };
            }
        "#,
        |_, module, receipt, typed| {
            let field = |record, name| canonical_record_field(typed, &module.key, record, name);
            let lower = |name, limits| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &declaration.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            let joined = lower("join", default);
            let mir = joined.mir().unwrap_or_else(|| {
                panic!(
                    "nested child restore: {:?} at {:?}",
                    joined.completion(),
                    joined.blocked_at().map(|site| site.span.bytes())
                )
            });
            let (taken, kept) = branch_targets(mir);
            for (branch, moved) in [(taken, true), (kept, false)] {
                let ops = linear(mir, branch);
                let install = ops
                    .iter()
                    .position(|op| matches!(op, Operation::Install { .. }))
                    .unwrap();
                let [
                    Operation::Move {
                        from: fresh,
                        into: Some(staged),
                    },
                    Operation::Cleanup(displaced),
                    Operation::Install { target, incoming },
                    Operation::Move {
                        from: whole,
                        into: Some(output),
                    },
                    Operation::Export(exported),
                    ..,
                ] = &ops[install - 2..]
                else {
                    panic!("expected staging, displacement, install, then whole export")
                };
                assert_eq!(fresh.root.span.bytes(), b"fresh: Inner");
                assert!(
                    fresh.projection.is_none()
                        && incoming.projection.is_none()
                        && whole.projection.is_none()
                );
                assert_eq!(staged.root.node, incoming.root.node);
                assert_eq!(target.root.span.bytes(), b"saved");
                assert_eq!(target.root.node, whole.root.node);
                assert_eq!(&target.field().unwrap().id, field("Outer", "child"));
                assert_eq!(output.root.node, exported.root.node);
                let drops = path_drops(mir, branch);
                assert_eq!(drops.len(), 2, "drop exactly the old child once");
                if moved {
                    assert!(
                        displaced.is_empty(),
                        "a moved child owes no displacement cleanup"
                    );
                    for (drop, name) in drops.iter().zip(["last", "first"]) {
                        assert!(
                            matches!(drop, Obligation::Drop(place) if place.root.span.bytes() == b"gone" && place.field().is_some_and(|member| &member.id == field("Inner", name)))
                        );
                    }
                } else {
                    assert!(
                        matches!(displaced.as_slice(), [Obligation::Drop(_), Obligation::EndStorage(marker), Obligation::Drop(_), Obligation::EndStorage(header)]
                        if header.root.node == target.root.node && header.field().is_some_and(|member| &member.id == field("Outer", "child"))
                            && marker.root.node == target.root.node && matches!(marker.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", "child") && &inner.id == field("Inner", "flag")))
                    );
                    for (drop, name) in drops.iter().zip(["last", "first"]) {
                        assert!(
                            matches!(drop, Obligation::Drop(place) if place.root.node == target.root.node
                            && matches!(place.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", "child") && &inner.id == field("Inner", name)))
                        );
                    }
                }
            }

            let restored = lower("loopRestore", default);
            let mir = restored
                .mir()
                .expect("the child is initialized at every backedge");
            let (body, exit) = branch_targets(mir);
            let path = straight_blocks(mir, body);
            let mut region = path.iter().filter_map(|block| match &block.operation {
                Operation::Move {
                    from,
                    into: Some(dest),
                } => Some((from, dest)),
                _ => None,
            });
            let (child, taken) = region.next().unwrap();
            let (source, staged) = region.next().unwrap();
            assert!(region.next().is_none());
            assert_eq!(&child.field().unwrap().id, field("Outer", "child"));
            assert_eq!(taken.root.span.bytes(), b"taken");
            assert_eq!(taken.root.node, source.root.node);
            let install = path
                .iter()
                .position(|block| matches!(block.operation, Operation::Install { .. }))
                .unwrap();
            assert!(
                matches!(&path[install - 1].operation, Operation::Cleanup(actions) if actions.is_empty())
            );
            assert!(
                matches!(path[install].operation, Operation::Install { target, incoming }
                if target.root.node == child.root.node && target.field().is_some_and(|member| &member.id == field("Outer", "child")) && incoming.root.node == staged.root.node)
            );
            let loop_test = straight_blocks(mir, mir.entry);
            assert!(std::ptr::eq(
                *path.last().unwrap(),
                *loop_test.last().unwrap()
            ));
            assert!(!path.iter().any(|block| {
                match &block.operation {
                    Operation::Drop(_) => true,
                    Operation::Cleanup(actions) => actions
                        .iter()
                        .any(|action| matches!(action, Obligation::Drop(_))),
                    _ => false,
                }
            }));
            assert!(path_drops(mir, exit).is_empty());
            let route: Vec<_> = linear(mir, exit)
                .into_iter()
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::Export(_)))
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Move { from, into: Some(staged) }, Operation::Export(output)]
                if from.root.node == child.root.node && staged.root.node == output.root.node
                    && [from, staged, output].iter().all(|place| place.projection.is_none()))
            );

            let skipped = lower("skipRestore", default);
            assert_eq!(skipped.completion(), Completion::SourceRejected);
            assert!(skipped.mir().is_none());
            let [finding] = skipped.findings() else {
                panic!("expected one backedge initialization finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"saved.child");
            assert_eq!(finding.related.node, finding.site.node);
            let one_edge = lower("repairOneEdge", default);
            assert_eq!(one_edge.completion(), Completion::SourceRejected);
            assert!(one_edge.mir().is_none());
            let [finding] = one_edge.findings() else {
                panic!("expected a whole read rejection on the unrepaired edge")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"saved");
            assert_eq!(finding.related.span.bytes(), b"saved.child");
            assert!(finding.related.span.start() < finding.site.span.start());
            for (name, site) in [
                ("movedRoot", b"saved.child = fresh;".as_slice()),
                ("deep", b"saved.child.first"),
                ("scalar", b"saved.sibling"),
                ("projected", b"fresh.child"),
                (
                    "construct",
                    b"Inner { first: first, flag: false, last: last }",
                ),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete);
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site);
            }
            let exact = AnalysisLimits {
                max_work: restored.statistics().work,
                max_entries: restored.statistics().entries,
                ..default
            };
            assert!(lower("loopRestore", exact).mir().is_some());
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
                let limited = lower("loopRestore", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn owned_nested_record_pattern_transfers_exact_leaves_and_cleans_the_remainder() {
    use compiler::analysis::ownership::Projection;

    probe_with_typed(
        "records::owned_nested_record_pattern_transfers_exact_leaves_and_cleans_the_remainder",
        r#"
            type Inner = { first: string; keep: string; flag: bool; last: string; }
            type Outer = { earlier: Inner; child: Inner; rest: string; suffix: string; }
            function unpack(value: Outer): string {
                let saved = value;
                let Outer { suffix: tail, child: Inner { last: (selected), keep: _, flag } } = (saved);
                return selected;
            }
            function partial(value: Outer): string {
                let gone = value.earlier;
                let Outer { child: Inner { last: selected } } = value;
                return selected;
            }
            function reuse(value: Outer): string {
                let Outer { child: Inner { last: selected } } = value;
                return value.rest;
            }
            type Bits = { marked: bool; }
            type Flags = { child: Bits; ready: bool; }
            function copyable(value: Flags): bool {
                let Flags { child: Bits { marked } } = value;
                if (marked) { return value.ready; }
                return false;
            }
            function borrowed(value: &Flags): bool {
                let Flags { child: Bits { marked } } = value;
                return marked;
            }
        "#,
        |_, module, receipt, typed| {
            let field = |record, name| canonical_record_field(typed, &module.key, record, name);
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            let unpack = lower("unpack", default);
            let mir = unpack.mir().unwrap_or_else(|| {
                panic!(
                    "nested pattern: {:?} at {:?}",
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
                            | Operation::Export(_)
                    )
                })
                .collect();
            let [
                Operation::Move {
                    from: value,
                    into: Some(saved),
                },
                Operation::Move {
                    from: original,
                    into: Some(scratch),
                },
                Operation::Move {
                    from: suffix,
                    into: Some(tail),
                },
                Operation::Move {
                    from: leaf,
                    into: Some(selected),
                },
                Operation::Copy {
                    from: marker,
                    into: Some(flag),
                },
                Operation::Cleanup(remaining),
                Operation::Move {
                    from: result,
                    into: Some(staged),
                },
                Operation::Export(output),
                ..,
            ] = route.as_slice()
            else {
                panic!(
                    "expected one stage, transfers in pattern order, temporary cleanup, then export"
                )
            };
            assert_eq!(value.root.span.bytes(), b"value: Outer");
            assert_eq!(saved.root.span.bytes(), b"saved");
            assert_eq!(original.root.node, saved.root.node);
            assert_eq!(scratch.root.span.bytes(), b"(saved)");
            assert!(
                [value, saved, original, scratch]
                    .iter()
                    .all(|place| place.projection.is_none())
            );
            let nested = |place: &compiler::analysis::ownership::Place<'_>,
                          outer_name,
                          inner_name| {
                place.root.node == scratch.root.node
                    && matches!(place.projection,
                    Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", outer_name) && &inner.id == field("Inner", inner_name))
            };
            assert_eq!(suffix.root.node, scratch.root.node);
            assert_eq!(&suffix.field().unwrap().id, field("Outer", "suffix"));
            assert_eq!(tail.root.span.bytes(), b"tail");
            assert!(nested(leaf, "child", "last"));
            assert!(nested(marker, "child", "flag"));
            assert_eq!(selected.root.span.bytes(), b"selected");
            assert_eq!(flag.root.span.bytes(), b"flag");
            assert_eq!(result.root.node, selected.root.node);
            assert_eq!(output.root.node, staged.root.node);
            let drops = path_drops(mir, mir.entry);
            assert_eq!(drops.len(), 7);
            assert!(
                matches!(drops[0], Obligation::Drop(rest) if rest.root.node == scratch.root.node
                && rest.field().is_some_and(|member| &member.id == field("Outer", "rest")))
            );
            assert!(
                matches!(drops[6], Obligation::Drop(name) if name.root.node == tail.root.node && name.projection.is_none())
            );
            for (drop, (outer, inner)) in drops[1..6].iter().zip([
                ("child", "keep"),
                ("child", "first"),
                ("earlier", "last"),
                ("earlier", "keep"),
                ("earlier", "first"),
            ]) {
                assert!(matches!(drop, Obligation::Drop(place) if nested(place, outer, inner)));
            }
            assert_eq!(
                remaining
                    .iter()
                    .filter(|action| matches!(action, Obligation::Drop(_)))
                    .count(),
                6
            );
            let copied = lower("copyable", default);
            let mir = copied
                .mir()
                .expect("copyable patterns preserve their original source");
            let prefix = straight_blocks(mir, mir.entry);
            assert!(prefix.iter().any(|block| matches!(block.operation, Operation::Copy { from, into: Some(dest) }
                if from.root.span.bytes() == b"value: Flags" && from.projection.is_none() && dest.root.span.bytes() == b"value")));
            let (yes, _) = branch_targets(mir);
            assert!(linear(mir, yes).iter().any(|op| matches!(op, Operation::Copy { from, .. }
                if from.root.span.bytes() == b"value: Flags" && from.field().is_some_and(|member| &member.id == field("Flags", "ready")))));
            for (name, site, related) in [
                ("partial", b"value".as_slice(), b"value.earlier".as_slice()),
                ("reuse", b"value.rest", b"value"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected one moved storage finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), related);
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            let borrowed = lower("borrowed", default);
            assert_eq!(borrowed.completion(), Completion::Incomplete);
            assert!(borrowed.mir().is_none());
            assert_eq!(
                borrowed.blocked_at().unwrap().span.bytes(),
                b"Bits { marked }"
            );
            let exact = AnalysisLimits {
                max_work: unpack.statistics().work,
                max_entries: unpack.statistics().entries,
                ..default
            };
            assert!(lower("unpack", exact).mir().is_some());
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
                assert_eq!(report.completion(), Completion::ResourceLimit);
                assert!(report.mir().is_none());
            }
        },
    );
}

#[test]
fn owned_record_projections() {
    probe(
        "records::owned_record_projections",
        r#"
            import std::core::dispose as release;
            type Pack = { first: string; second: string; third: string; }
            function splitField(flag: bool, pack: Pack): string {
                if (flag) { let taken = pack.first; }
                return pack.second;
            }
            function movedField(flag: bool, pack: Pack): string {
                if (flag) { let taken = pack.first; }
                return pack.first;
            }
            function movedWhole(flag: bool, pack: Pack): Pack {
                if (flag) { let taken = pack.first; }
                return pack;
            }
            function construct(one: string, two: string, three: string): void {
                let pack = Pack { third: three, first: one, second: two };
            }
            function restoreField(seed: Pack): string {
                let mut pack = seed;
                let taken = pack.first;
                pack.first = "new";
                return taken;
            }
            function replaceField(seed: Pack): void {
                let mut pack = seed;
                pack.second = "new";
            }
            function disjointFields(seed: Pack): string {
                let mut pack = seed;
                let access = &mut pack.first;
                let taken = pack.second;
                let last = access;
                return taken;
            }
            function sameField(seed: Pack): void {
                let mut pack = seed;
                let access = &mut pack.first;
                pack.first = "new";
                let last = access;
            }
            function temporaryField(): string {
                return (Pack { first: "a", second: "b", third: "c" }).first;
            }
            function overlappingWhole(pack: Pack): string {
                let access = &pack;
                let taken = pack.second;
                let last = access;
                return taken;
            }
            function escapingField(pack: Pack): &string {
                return &pack.first;
            }
            function replacePartial(seed: Pack, next: Pack): string {
                let mut pack = seed;
                let taken = pack.first;
                pack = next;
                return pack.second;
            }
            type CustomPack = { text: string; remaining: string; }
            implement release(value: CustomPack): void { }
            function customPartial(pack: CustomPack): string { return pack.text; }
            function customConditional(flag: bool, pack: CustomPack): void {
                if (flag) { let before = true; let taken = pack.text; }
            }
            function customWhole(pack: CustomPack): CustomPack { return pack; }
            function customCleanup(pack: CustomPack): void { }
            function borrowedFieldMove(pack: &Pack): string { return pack.first; }
            type FlagPack = { bit: bool; }
            function copyRecord(pack: FlagPack): bool {
                let other = pack;
                let original = pack.bit;
                return other.bit;
            }
        "#,
        |request, module, receipt| {
            let lower = |name| {
                let id = &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id;
                lower_body(receipt, module, id, AnalysisLimits::default()).unwrap()
            };
            let field_split = lower("splitField");
            let mir = field_split.mir().unwrap_or_else(|| {
                panic!(
                    "{:?}: {:?}",
                    field_split.completion(),
                    field_split
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                )
            });
            let (yes, no) = branch_targets(mir);
            let drop_names = |start| {
                path_drops(mir, start)
                    .iter()
                    .map(|drop| match drop {
                        Obligation::Drop(place) => (
                            place.root.span.bytes(),
                            place.field().map(|field| field.name.as_deref().unwrap()),
                        ),
                        other => panic!("unexpected cleanup {other:?}"),
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                drop_names(yes),
                [
                    (b"taken".as_slice(), None),
                    (b"pack: Pack".as_slice(), Some("third"))
                ]
            );
            assert_eq!(
                drop_names(no),
                [
                    (b"pack: Pack".as_slice(), Some("third")),
                    (b"pack: Pack".as_slice(), Some("first"))
                ]
            );
            for (name, use_site) in [
                ("movedField", b"pack.first".as_slice()),
                ("movedWhole", b"pack".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected);
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("expected one finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), use_site);
                assert_eq!(finding.related.span.bytes(), b"pack.first");
            }
            let constructed = lower("construct");
            let mir = constructed
                .mir()
                .unwrap_or_else(|| panic!("{:?}", constructed.completion()));
            let fields: Vec<_> = mir
                .blocks
                .iter()
                .filter_map(|b| match b.operation {
                    Operation::Move {
                        into: Some(place), ..
                    } => place.field().and_then(|field| field.name.as_deref()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                fields,
                ["third", "first", "second"],
                "evaluate fields as written"
            );
            assert_eq!(
                dropped_fields(mir),
                ["third", "second", "first"],
                "clean in reverse declaration order"
            );
            let restored = lower("restoreField");
            let mir = restored
                .mir()
                .unwrap_or_else(|| panic!("{:?}", restored.completion()));
            assert_eq!(
                dropped_fields(mir),
                ["third", "second", "first"],
                "do not drop the moved old field again"
            );
            let replaced = lower("replaceField");
            let mir = replaced
                .mir()
                .unwrap_or_else(|| panic!("{:?}", replaced.completion()));
            assert_eq!(dropped_fields(mir), ["second", "third", "second", "first"]);
            assert!(mir.blocks.iter().any(|block| matches!(( &block.operation, block.terminator), (Operation::Drop(place), Terminator::Goto(next))
            if place.field().is_some_and(|f| f.name.as_deref() == Some("second"))
                && matches!(mir.blocks[next].operation, Operation::Install { target, .. } if target.field().is_some_and(|f| f.name.as_deref() == Some("second"))))));
            let disjoint = lower("disjointFields");
            let mir = disjoint
                .mir()
                .unwrap_or_else(|| panic!("{:?}", disjoint.completion()));
            assert_eq!(dropped_fields(mir), ["third", "first"]);
            let replaced = lower("replacePartial");
            let mir = replaced
                .mir()
                .unwrap_or_else(|| panic!("{:?}", replaced.completion()));
            assert_eq!(dropped_fields(mir), ["third", "second", "third", "first"]);
            let temporary = lower("temporaryField");
            let mir = temporary
                .mir()
                .unwrap_or_else(|| panic!("{:?}", temporary.completion()));
            assert_eq!(dropped_fields(mir), ["third", "second"]);
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Move { from, into: Some(_) }
                if from.field().is_some_and(|field| field.name.as_deref() == Some("first")))
            ));
            let copy = lower("copyRecord");
            let mir = copy
                .mir()
                .unwrap_or_else(|| panic!("{:?}", copy.completion()));
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Copy { from, into: Some(into) }
                if from.projection.is_none() && from.root.span.bytes() == b"pack: FlagPack"
                    && into.root.span.bytes() == b"other")
            ));
            assert!(path_drops(mir, mir.entry).is_empty());
            let same = lower("sameField");
            assert_eq!(same.completion(), Completion::SourceRejected);
            assert!(same.mir().is_none());
            let [finding] = same.findings() else {
                panic!("expected one finding")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.related.span.bytes(), b"&mut pack.first");
            let transferred = lower("customWhole");
            let mir = transferred
                .mir()
                .unwrap_or_else(|| panic!("{:?}", transferred.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let checked = lower("customCleanup");
            let mir = checked
                .mir()
                .expect("checked outer disposer and default field cleanup");
            let operations = path_drops(mir, mir.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = operations.as_slice() else {
                panic!("expected only an outer disposer at the caller boundary: {operations:?}");
            };
            let active = mir.disposer(implementation).unwrap();
            assert_eq!(dropped_fields(active), ["remaining", "text"]);

            for (name, code) in [
                ("overlappingWhole", "R0203"),
                ("escapingField", "R0202"),
                ("customPartial", "R0501"),
                ("customConditional", "R0501"),
                ("borrowedFieldMove", "R0501"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert_eq!(report.findings()[0].code(), code, "{name}");
                assert!(report.mir().is_none());
            }

            let duplicate = request.entry.with_file_name("duplicate.rill");
            let mut source = fs::read_to_string(&request.entry).unwrap();
            source.push_str("\nimplement dispose(other: CustomPack): void { }\n");
            fs::write(&duplicate, source).unwrap();
            let graph = loader::load_installed(InstalledRequest {
                entry: duplicate,
                ..request.clone()
            })
            .unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            assert!(typed.receipt().is_none());
            assert!(
                typed
                    .bodies()
                    .findings()
                    .iter()
                    .any(|f| f.kind.code() == Some("R0304"))
            );
        },
    );
}

fn dropped_fields<'g>(mir: &Mir<'g>) -> Vec<&'g str> {
    path_drops(mir, mir.entry)
        .into_iter()
        .filter_map(|drop| match drop {
            Obligation::Drop(place) => place.field().and_then(|field| field.name.as_deref()),
            _ => None,
        })
        .collect()
}
