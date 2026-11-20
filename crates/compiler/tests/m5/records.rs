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
            let constructed = lower("construct", default);
            let mir = constructed
                .mir()
                .expect("a named initializer can build the admitted nested layout");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(sibling), Obligation::Drop(text)]
                if sibling.root.span.bytes() == b"value" && sibling.root.node == text.root.node
                    && sibling.field().is_some_and(|member| &member.id == field("Outer", "sibling"))
                    && matches!(text.projection, Some(Projection::NestedField { outer, inner })
                        if &outer.id == field("Outer", "child") && &inner.id == field("Inner", "text")))
            );
            for (name, at) in [
                ("borrow", b"value".as_slice()),
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
fn replacing_an_initialized_nested_string_leaf_preserves_its_neighbors() {
    use compiler::analysis::TypeTerm;
    use compiler::analysis::data::{Access, PathProjection};
    use compiler::analysis::ownership::{Place, Projection};
    use compiler::analysis::scalars::{NodeFactKind, Place as TypedPlace};
    use compiler::analysis::typed::TypedCompletion;
    use compiler::parser::NodeKind;

    let mut failures = Vec::new();
    probe_with_typed(
        "records::replacing_an_initialized_nested_string_leaf_preserves_its_neighbors",
        r#"
            type Inner = { first: string; flag: bool; last: string; }
            type Outer = { child: Inner; sibling: string; }
            function replace(seed: Outer, fresh: string): bool {
                let mut saved = seed;
                saved.child.first = fresh;
                return true;
            }
            function movedChild(seed: Outer, fresh: string): void {
                let mut saved = seed;
                let taken = saved.child;
                saved.child.first = fresh;
            }
            function movedRoot(seed: Outer, fresh: string): void {
                let mut saved = seed;
                let taken = saved;
                saved.child.first = fresh;
            }
        "#,
        |request, module, receipt, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Checked);
            let field = |record, name| canonical_record_field(typed, &module.key, record, name);
            let child = field("Outer", "child");
            let first = field("Inner", "first");
            for assignment in module
                .syntax
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Assignment("="))
            {
                let expression =
                    |node| match &typed.bodies().node(module, node).unwrap().unwrap().kind {
                        NodeFactKind::Expression(facts) => facts,
                        _ => panic!("expected typed assignment operand"),
                    };
                let lhs = expression(assignment.children[0]);
                let rhs = expression(assignment.children[1]);
                assert_eq!(lhs.place, TypedPlace::Writable);
                assert_eq!(lhs.access, Access::Owned);
                assert_eq!(rhs.access, Access::Owned);
                let path = lhs.path.as_ref().unwrap();
                assert_eq!(path.root.kind, loader::DeclarationKind::Binding);
                assert_eq!(
                    path.projections,
                    [PathProjection::Field(child), PathProjection::Field(first)]
                );
                let source = rhs.path.as_ref().unwrap();
                assert_eq!(source.root.kind, loader::DeclarationKind::Parameter);
                assert!(source.projections.is_empty());
                assert!(matches!(lhs.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "string"));
                assert_eq!(lhs.ty, rhs.ty);
            }

            for (name, moved_at) in [
                ("replace", None),
                ("movedChild", Some(b"saved.child".as_slice())),
                ("movedRoot", Some(b"saved".as_slice())),
            ] {
                let expected = if moved_at.is_some() {
                    Completion::SourceRejected
                } else {
                    Completion::Checked
                };
                let declaration = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &declaration.id, AnalysisLimits::default())
                        .unwrap();
                if report.completion() != expected {
                    failures.push(format!(
                        "test_mode={}: M4 {:?}; {name}: expected {expected:?}, got {:?} at {:?}; MIR={}, findings={:?}",
                        request.test_mode, typed.completion(), report.completion(),
                        report.blocked_at().map(|site| site.span.bytes().escape_ascii().to_string()),
                        report.mir().is_some(), report.findings().iter().map(|finding| finding.code()).collect::<Vec<_>>()
                    ));
                    continue;
                }
                if let Some(moved_at) = moved_at {
                    assert!(report.mir().is_none());
                    let [finding] = report.findings() else {
                        panic!("expected one moved-ancestor finding")
                    };
                    assert_eq!(finding.code(), "R0200");
                    assert_eq!(finding.site.span.bytes(), b"saved.child.first = fresh;");
                    assert_eq!(finding.related.span.bytes(), moved_at);
                    assert!(finding.related.span.start() < finding.site.span.start());
                    continue;
                }

                let mir = report.mir().unwrap();
                let ops = linear(mir, mir.entry);
                let from_fresh = |op: &&Operation<'_>| {
                    matches!(op, Operation::Move { from, .. }
                    if from.root.span.bytes() == b"fresh: string" && from.projection.is_none())
                };
                assert_eq!(ops.iter().filter(|op| from_fresh(op)).count(), 1);
                let stage = ops.iter().position(from_fresh).unwrap();
                let [
                    Operation::Move {
                        into: Some(staged), ..
                    },
                    Operation::Drop(old),
                    Operation::Install { target, incoming },
                    ..,
                ] = &ops[stage..]
                else {
                    panic!("expected staging, one old leaf drop, then install: {ops:?}");
                };
                assert_eq!(staged.root.node, incoming.root.node);
                assert!(staged.projection.is_none() && incoming.projection.is_none());
                assert_eq!(target.root.span.bytes(), b"saved");
                let nested = |place: &Place<'_>, member| {
                    place.root.node == target.root.node
                        && matches!(place.projection, Some(Projection::NestedField { outer, inner })
                        if &outer.id == child && &inner.id == member)
                };
                assert!(nested(target, first) && nested(old, first));
                assert_eq!(
                    ops.iter()
                        .filter(|op| matches!(op, Operation::Install { .. }))
                        .count(),
                    1
                );

                let export = ops
                    .iter()
                    .position(|op| matches!(op, Operation::Export(_)))
                    .unwrap();
                assert!(stage + 2 < export);
                assert!(ops[export + 1..].iter().any(|op| matches!(op, Operation::Cleanup(actions)
                    if matches!(actions.as_slice(), [Obligation::Drop(sibling), Obligation::Drop(last), Obligation::EndStorage(flag), Obligation::Drop(replacement), ..]
                        if sibling.root.node == target.root.node && sibling.field().is_some_and(|decl| &decl.id == field("Outer", "sibling"))
                            && nested(last, field("Inner", "last")) && nested(flag, field("Inner", "flag")) && nested(replacement, first)))));
                assert_eq!(
                    path_drops(mir, mir.entry).len(),
                    4,
                    "no extra drops before or after install"
                );
            }
        },
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
            function ignore(value: Outer): void {
                let Outer { child: Inner { last: _ } } = value;
            }
            function wholeChild(value: Outer): string {
                let Outer { earlier: whole, child: Inner { last: selected } } = value;
                return selected;
            }
            type Bits = { marked: bool; }
            type Flags = { child: Bits; ready: bool; }
            function constructed(value: Flags): bool {
                let Flags { child: Bits { marked } } = Flags { child: Bits { marked: true }, ready: false };
                return marked;
            }
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
                Operation::Cleanup(lexical),
                Operation::Cleanup(parameters),
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
            let direct = |place: &compiler::analysis::ownership::Place<'_>, name| {
                place.root.node == scratch.root.node
                    && place
                        .field()
                        .is_some_and(|member| &member.id == field("Outer", name))
            };
            assert!(matches!(remaining.as_slice(), [
                Obligation::Drop(rest),
                Obligation::EndStorage(marker), Obligation::Drop(keep), Obligation::Drop(first), Obligation::EndStorage(child),
                Obligation::Drop(last), Obligation::EndStorage(earlier_marker), Obligation::Drop(earlier_keep), Obligation::Drop(earlier_first), Obligation::EndStorage(earlier),
                Obligation::EndStorage(root),
            ] if direct(rest, "rest") && nested(marker, "child", "flag") && nested(keep, "child", "keep")
                && nested(first, "child", "first") && direct(child, "child") && nested(last, "earlier", "last")
                && nested(earlier_marker, "earlier", "flag") && nested(earlier_keep, "earlier", "keep")
                && nested(earlier_first, "earlier", "first") && direct(earlier, "earlier")
                && root.root.node == scratch.root.node && root.projection.is_none()));
            assert!(
                matches!(lexical.as_slice(), [Obligation::EndStorage(marker), Obligation::Drop(name)]
                if marker.root.node == flag.root.node && name.root.node == tail.root.node
                    && marker.projection.is_none() && name.projection.is_none())
            );
            assert!(parameters.is_empty());

            let ignored = lower("ignore", default);
            let mir = ignored
                .mir()
                .expect("wildcards still consume the noncopyable source");
            let stages: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::Copy { .. }))
                .collect();
            let [
                Operation::Move {
                    from,
                    into: Some(discarded),
                },
            ] = stages.as_slice()
            else {
                panic!("expected one move into the wildcard temporary");
            };
            assert_eq!(from.root.span.bytes(), b"value: Outer");
            assert!(from.projection.is_none() && discarded.projection.is_none());
            let dropped = path_drops(mir, mir.entry);
            assert_eq!(dropped.len(), 8);
            assert!(dropped.iter().all(|drop| matches!(drop, Obligation::Drop(place) if place.root.node == discarded.root.node && place.projection.is_some())));
            let copied = lower("copyable", default);
            let mir = copied
                .mir()
                .expect("copyable patterns preserve their original source");
            let prefix = straight_blocks(mir, mir.entry);
            let copies: Vec<_> = prefix
                .iter()
                .filter_map(|block| match &block.operation {
                    Operation::Copy {
                        from,
                        into: Some(dest),
                    } => Some((from, dest)),
                    _ => None,
                })
                .collect();
            assert!(
                matches!(copies.as_slice(), [(value, scratch), (leaf, marked)]
                if value.root.span.bytes() == b"value: Flags" && value.projection.is_none()
                    && scratch.root.span.bytes() == b"value" && scratch.projection.is_none()
                    && leaf.root.node == scratch.root.node && marked.root.span.bytes() == b"marked"
                    && matches!(leaf.projection, Some(Projection::NestedField { outer, inner })
                        if &outer.id == field("Flags", "child") && &inner.id == field("Bits", "marked")))
            );
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
            for (name, site) in [
                ("borrowed", b"Bits { marked }".as_slice()),
                ("wholeChild", b"whole"),
                (
                    "constructed",
                    b"Flags { child: Bits { marked: true }, ready: false }",
                ),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete);
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site);
            }
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
fn nested_record_initializers_transfer_written_fields_into_canonical_storage() {
    use compiler::analysis::ownership::Projection;

    probe_with_typed(
        "records::nested_record_initializers_transfer_written_fields_into_canonical_storage",
        r#"
            type Inner = { first: string; mark: bool; last: string; }
            type Outer = { first: Inner; sibling: string; last: Inner; }
            function cleanup(early: Inner, late: Inner, text: string): void {
                let saved = (Outer { last: (late), sibling: text, first: early });
            }
            function split(early: Inner, late: Inner): string {
                let saved = Outer { last: late, sibling: "sibling", first: early };
                let taken = saved.last;
                return saved.sibling;
            }
            function passParts(parts: [string, string]): [string, string] { return parts; }
            function projected(early: Inner, late: Inner, parts: [string, string]): void {
                let saved = Outer { last: late, sibling: passParts(parts)[1], first: early };
            }
            function duplicate(seed: Inner, text: string): void {
                let saved = Outer { last: seed, sibling: text, first: seed };
            }
            function partial(early: Inner, late: Inner): void {
                let gone = late.last;
                let saved = Outer { first: early, sibling: "sibling", last: late };
            }
            function returned(early: Inner, late: Inner): Outer {
                return Outer { first: early, sibling: "sibling", last: late };
            }
            function inline(early: Inner, text: string): void {
                let saved = Outer { first: early, sibling: "sibling", last: Inner { first: text, mark: true, last: "last" } };
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
            let clean = lower("cleanup", default);
            let mir = clean.mir().unwrap_or_else(|| {
                panic!(
                    "named nested construction: {:?} at {:?}",
                    clean.completion(),
                    clean.blocked_at().map(|site| site.span.bytes())
                )
            });
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::Copy { .. }
                            | Operation::Initialize(_)
                            | Operation::Cleanup(_)
                            | Operation::Drop(_)
                    )
                })
                .collect();
            let [
                Operation::Move {
                    from: late,
                    into: Some(last),
                },
                Operation::Move {
                    from: text,
                    into: Some(sibling),
                },
                Operation::Move {
                    from: early,
                    into: Some(first),
                },
                Operation::Initialize(owner),
                Operation::Cleanup(remaining),
                Operation::Cleanup(parameters),
            ] = route.as_slice()
            else {
                panic!(
                    "expected field moves in written order, root initialization, then lexical cleanup"
                )
            };
            assert_eq!(late.root.span.bytes(), b"late: Inner");
            assert_eq!(text.root.span.bytes(), b"text: string");
            assert_eq!(early.root.span.bytes(), b"early: Inner");
            assert_eq!(owner.root.span.bytes(), b"saved");
            assert!(
                owner.projection.is_none()
                    && [late, text, early]
                        .iter()
                        .all(|place| place.projection.is_none())
            );
            for (place, name) in [(last, "last"), (sibling, "sibling"), (first, "first")] {
                assert_eq!(place.root.node, owner.root.node);
                assert_eq!(&place.field().unwrap().id, field("Outer", name));
            }
            let child = |place: &compiler::analysis::ownership::Place<'_>, parent, leaf| {
                place.root.node == owner.root.node
                    && matches!(place.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", parent) && &inner.id == field("Inner", leaf))
            };
            let direct = |place: &compiler::analysis::ownership::Place<'_>, name| {
                place.root.node == owner.root.node
                    && place
                        .field()
                        .is_some_and(|member| &member.id == field("Outer", name))
            };
            assert!(
                matches!(remaining.as_slice(), [Obligation::Drop(last), Obligation::EndStorage(last_bit), Obligation::Drop(last_first), Obligation::EndStorage(last_header), Obligation::Drop(sibling), Obligation::Drop(first_last), Obligation::EndStorage(first_bit), Obligation::Drop(first), Obligation::EndStorage(first_header), Obligation::EndStorage(root)]
                if child(last, "last", "last") && child(last_bit, "last", "mark") && child(last_first, "last", "first") && direct(last_header, "last")
                    && direct(sibling, "sibling") && child(first_last, "first", "last") && child(first_bit, "first", "mark")
                    && child(first, "first", "first") && direct(first_header, "first") && root.root.node == owner.root.node && root.projection.is_none())
            );
            assert!(
                parameters.is_empty(),
                "the incoming fields transferred into the named local"
            );
            let projected = lower("projected", default);
            let mir = projected.mir().unwrap_or_else(|| {
                panic!(
                    "temporary primitive field: {:?} at {:?}",
                    projected.completion(),
                    projected.blocked_at().map(|site| site.span.bytes())
                )
            });
            let ops: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .filter(|block| {
                    matches!(
                        block.operation,
                        Operation::Move { .. }
                            | Operation::Call(_)
                            | Operation::Cleanup(_)
                            | Operation::Initialize(_)
                    )
                })
                .collect();
            let [last_move, argument, call, read, end, first_move, root, _, _] = ops.as_slice()
            else {
                panic!("expected one temporary call and remainder cleanup in written field order")
            };
            let Operation::Initialize(owner) = root.operation else {
                panic!("expected the named record to initialize last")
            };
            assert!(
                matches!(last_move.operation, Operation::Move { from, into: Some(to) }
                if from.root.span.bytes() == b"late: Inner" && to.root.node == owner.root.node && to.field().is_some_and(|member| &member.id == field("Outer", "last")))
            );
            assert!(
                matches!(argument.operation, Operation::Move { from, .. } if from.root.span.bytes() == b"parts: [string, string]")
            );
            assert!(
                matches!(call.operation, Operation::Call(callee) if callee == &module.declarations.iter().find(|decl| decl.name.as_deref() == Some("passParts")).unwrap().id)
            );
            assert!(
                matches!(read.operation, Operation::Move { from, into: Some(to) }
                if from.root.node == call.site.node && from.tuple_element() == Some(1) && to.root.node == owner.root.node
                    && to.field().is_some_and(|member| &member.id == field("Outer", "sibling")))
            );
            assert!(matches!(&end.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::Drop(remaining), Obligation::EndStorage(root)]
                    if remaining.root.node == call.site.node && remaining.tuple_element() == Some(0)
                        && root.root.node == call.site.node && root.projection.is_none())));
            assert!(
                matches!(first_move.operation, Operation::Move { from, into: Some(to) }
                if from.root.span.bytes() == b"early: Inner" && to.root.node == owner.root.node && to.field().is_some_and(|member| &member.id == field("Outer", "first")))
            );

            let split = lower("split", default);
            let mir = split.mir().expect("direct reads work after initialization");
            let ops = linear(mir, mir.entry);
            let export = ops
                .iter()
                .position(|op| matches!(op, Operation::Export(_)))
                .unwrap();
            assert!(ops[export + 1..].iter().any(|op| matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action, Obligation::Drop(place) if place.root.span.bytes() == b"taken")))));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(a), Obligation::Drop(b), Obligation::Drop(c), Obligation::Drop(d)]
                if a.root.span.bytes() == b"taken" && b.root.node == a.root.node && c.root.span.bytes() == b"saved" && d.root.node == c.root.node
                    && a.field().is_some_and(|member| &member.id == field("Inner", "last")) && b.field().is_some_and(|member| &member.id == field("Inner", "first"))
                    && matches!(c.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", "first") && &inner.id == field("Inner", "last"))
                    && matches!(d.projection, Some(Projection::NestedField { outer, inner }) if &outer.id == field("Outer", "first") && &inner.id == field("Inner", "first")))
            );
            for (name, site, related) in [
                ("duplicate", b"seed".as_slice(), b"seed".as_slice()),
                ("partial", b"late", b"late.last"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected a moved initializer finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), related);
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            for (name, site) in [
                (
                    "returned",
                    b"Outer { first: early, sibling: \"sibling\", last: late }".as_slice(),
                ),
                (
                    "inline",
                    b"Inner { first: text, mark: true, last: \"last\" }",
                ),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete);
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site);
            }
            let exact = AnalysisLimits {
                max_work: clean.statistics().work,
                max_entries: clean.statistics().entries,
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
                let report = lower("cleanup", short);
                assert_eq!(report.completion(), Completion::ResourceLimit);
                assert!(report.mir().is_none());
            }
        },
    );
}

#[test]
fn named_nested_record_cleanup_after_one_edge_child_move() {
    use compiler::analysis::{
        ownership::{Place, Projection},
        scalars::NodeFactKind,
    };
    use compiler::parser::NodeKind;

    probe_with_typed(
        "records::named_nested_record_cleanup_after_one_edge_child_move",
        r#"
            import std::core::dispose as release;
            type Inner = { text: string; }
            type Outer = { child: Inner; sibling: string; }
            function defaultCleanup(flag: bool, incoming: Inner, text: string): void {
                let saved = Outer { sibling: text, child: incoming };
                if (flag) { let taken = saved.child; }
            }
            type CustomInner = { text: string; }
            type WithCustomChild = { child: CustomInner; sibling: string; }
            implement release(value: CustomInner): void {}
            function selectInner(inner: CustomInner): void { release(inner); }
            function innerDisposer(flag: bool, incoming: CustomInner, text: string): void {
                let saved = WithCustomChild { sibling: text, child: incoming };
                if (flag) { let taken = saved.child; }
            }
            type CustomOuter = { child: Inner; sibling: string; }
            implement release(value: CustomOuter): void {}
            function selectOuter(outer: CustomOuter): void { release(outer); }
            function outerDisposer(flag: bool, incoming: Inner, text: string): void {
                let saved = CustomOuter { sibling: text, child: incoming };
                if (flag) { let taken = saved.child; }
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
            let lower = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let report = lower("defaultCleanup");
            let mir = report.mir().expect("default nested cleanup after a split");
            let field = |record, name| canonical_record_field(typed, &module.key, record, name);
            let is_field = |place: &Place<'_>, root: &Place<'_>, id| {
                place.root.node == root.root.node && place.field().is_some_and(|f| &f.id == id)
            };
            let is_root = |place: &Place<'_>, root: &Place<'_>| {
                place.root.node == root.root.node && place.projection.is_none()
            };
            let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| !matches!(op, Operation::None | Operation::Copy { .. }))
                .collect();
            let [
                Operation::Move {
                    from: text,
                    into: Some(sibling),
                },
                Operation::Move {
                    from: incoming,
                    into: Some(child),
                },
                Operation::Initialize(saved),
            ] = prefix.as_slice()
            else {
                panic!("expected two field transfers and root initialization before the split")
            };
            assert_eq!(text.root.span.bytes(), b"text: string");
            assert_eq!(incoming.root.span.bytes(), b"incoming: Inner");
            assert_eq!(saved.root.span.bytes(), b"saved");
            assert!(
                [text, incoming, saved]
                    .iter()
                    .all(|p| p.projection.is_none())
            );
            assert!(is_field(sibling, saved, field("Outer", "sibling")));
            assert!(is_field(child, saved, field("Outer", "child")));

            let (yes, no) = branch_targets(mir);
            let effects = |entry| {
                let path = straight_blocks(mir, entry);
                assert_eq!(path.last().unwrap().terminator, Terminator::Return);
                path.into_iter()
                    .filter(|block| !matches!(block.operation, Operation::None))
                    .collect::<Vec<_>>()
            };
            let yes_path = effects(yes);
            let [transfer, arm_exit, moved_exit, yes_parameters] = yes_path.as_slice() else {
                panic!("expected the transfer, then arm, owner and parameter cleanup")
            };
            let Operation::Move {
                from: child,
                into: Some(taken),
            } = transfer.operation
            else {
                panic!("expected the child to move into the branch-local binding")
            };
            assert!(is_field(&child, saved, field("Outer", "child")));
            assert_eq!(taken.root.span.bytes(), b"taken");
            assert!(taken.projection.is_none());
            assert_eq!(arm_exit.site.span.bytes(), b"{ let taken = saved.child; }");
            assert!(matches!(&arm_exit.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::Drop(text), Obligation::EndStorage(root)]
                    if is_field(text, &taken, field("Inner", "text")) && is_root(root, &taken))));
            assert!(matches!(&moved_exit.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::Drop(sibling), Obligation::EndStorage(root)]
                    if is_field(sibling, saved, field("Outer", "sibling")) && is_root(root, saved))));

            let no_path = effects(no);
            let [whole_exit, no_parameters] = no_path.as_slice() else {
                panic!("expected only owner and parameter cleanup on the non-moving edge")
            };
            assert_eq!(whole_exit.site.node, moved_exit.site.node);
            assert!(whole_exit.site.span.start() < saved.root.span.start());
            assert!(arm_exit.site.span.end() < whole_exit.site.span.end());
            assert!(matches!(&whole_exit.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::Drop(sibling), Obligation::Drop(text), Obligation::EndStorage(child), Obligation::EndStorage(root)]
                    if is_field(sibling, saved, field("Outer", "sibling")) && is_field(child, saved, field("Outer", "child"))
                        && is_root(root, saved) && text.root.node == saved.root.node
                        && matches!(text.projection, Some(Projection::NestedField { outer, inner })
                            if &outer.id == field("Outer", "child") && &inner.id == field("Inner", "text")))));
            for end in [yes_parameters, no_parameters] {
                assert!(matches!(&end.operation, Operation::Cleanup(actions)
                    if matches!(actions.as_slice(), [Obligation::EndStorage(flag)]
                        if flag.root.span.bytes() == b"flag: bool" && flag.projection.is_none())));
            }

            for (name, call_span, implementation_span) in [
                (
                    "innerDisposer",
                    b"release(inner)".as_slice(),
                    b"implement release(value: CustomInner): void {}".as_slice(),
                ),
                (
                    "outerDisposer",
                    b"release(outer)",
                    b"implement release(value: CustomOuter): void {}",
                ),
            ] {
                let call_node = module
                    .syntax
                    .nodes
                    .iter()
                    .position(|node| node.kind == NodeKind::Call && node.span.bytes() == call_span)
                    .unwrap();
                let Some(NodeFactKind::Expression(call)) = typed
                    .bodies()
                    .node(module, call_node)
                    .unwrap()
                    .map(|fact| &fact.kind)
                else {
                    panic!("expected checked explicit dispose call: {name}");
                };
                let call = call.call.as_deref().unwrap();
                assert_eq!(
                    call.callee,
                    &typed.dispose_role().expect("canonical dispose").id
                );
                let selected = call
                    .selection
                    .as_ref()
                    .expect("exact disposer selection")
                    .implementation;
                let implementation = module
                    .declarations
                    .iter()
                    .find(|decl| &decl.id == selected)
                    .unwrap();
                assert_eq!(
                    module.syntax.nodes[implementation.node].span.bytes(),
                    implementation_span
                );

                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(
                    report.mir().is_none() && report.findings().is_empty(),
                    "{name}"
                );
                let blocker = report.blocked_at().expect("expected named storage blocker");
                let body = &module.syntax.nodes[declaration(name).node].span;
                assert_eq!(blocker.module, &module.key);
                assert!(body.start() <= blocker.span.start() && blocker.span.end() <= body.end());
                assert_eq!(blocker.span.bytes(), b"saved", "{name}");
                assert_eq!(
                    module.syntax.nodes[blocker.node].kind,
                    NodeKind::NamePattern
                );
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

#[test]
fn interrupted_record_initialization_cleans_only_the_reached_fields() {
    use compiler::analysis::{ownership::Place, scalars::NodeFactKind};
    use compiler::parser::NodeKind;

    probe_with_typed(
        "records::interrupted_record_initialization_cleans_only_the_reached_fields",
        r#"
            import std::core::dispose as release;
            type Pack = { first: string; check: bool; second: string; last: string; }
            type Custom = { first: string; check: bool; second: string; last: string; }
            implement release(value: Custom): void {}
            function select(value: Custom): void { release(value); }
            function interrupted(a: string, gate: Option<bool>, b: string, c: string, spare: string): Option<bool> {
                let kept = spare;
                let packed = Pack { second: c, first: a, check: gate?, last: b };
                return Option.Some(true);
            }
            function interruptedCustom(a: string, gate: Option<bool>, b: string, c: string, spare: string): Option<bool> {
                let kept = spare;
                let packed = Custom { second: c, first: a, check: (gate?), last: b };
                return Option.Some(true);
            }
            function movedPrefix(a: string, gate: Option<bool>, b: string, c: string): Option<bool> {
                let prior = a;
                let packed = Pack { first: a, second: c, check: gate?, last: b };
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let explicit = module
                .syntax
                .nodes
                .iter()
                .position(|node| {
                    node.kind == NodeKind::Call && node.span.bytes() == b"release(value)"
                })
                .unwrap();
            let NodeFactKind::Expression(call) =
                &typed.bodies().node(module, explicit).unwrap().unwrap().kind
            else {
                panic!("expected typed canonical disposer call");
            };
            let call = call.call.as_deref().unwrap();
            assert_eq!(call.callee, &typed.dispose_role().unwrap().id);
            let selected = call.selection.as_ref().unwrap().implementation;

            let role = typed.option_role().unwrap();
            let root = |place: &Place<'_>, bytes: &[u8]| {
                place.root.module == &module.key
                    && place.root.span.bytes() == bytes
                    && place.projection.is_none()
            };
            for (name, record) in [("interrupted", "Pack"), ("interruptedCustom", "Custom")] {
                let report = lower(name, AnalysisLimits::default());
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name} at {:?}",
                    report
                        .blocked_at()
                        .map(|site| site.span.bytes().escape_ascii().to_string())
                );
                let mir = report.mir().unwrap();
                let member = |place: &Place<'_>, bytes: &[u8], name| {
                    place.root.module == &module.key
                        && place.root.span.bytes() == bytes
                        && place.field().is_some_and(|field| {
                            &field.id == canonical_record_field(typed, &module.key, record, name)
                        })
                };
                let field = |place: &Place<'_>, name| member(place, b"packed", name);
                let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                    .into_iter()
                    .filter(|block| !matches!(block.operation, Operation::None))
                    .collect();
                let [kept, second, first, gate, split] = prefix.as_slice() else {
                    panic!("{name}: expected the written prefix and one carrier evaluation");
                };
                assert!(
                    matches!(kept.operation, Operation::Move { from, into: Some(into) } if root(&from, b"spare: string") && root(&into, b"kept"))
                );
                assert!(
                    matches!(second.operation, Operation::Move { from, into: Some(into) } if root(&from, b"c: string") && field(&into, "second"))
                );
                assert!(
                    matches!(first.operation, Operation::Move { from, into: Some(into) } if root(&from, b"a: string") && field(&into, "first"))
                );
                let Operation::Copy {
                    from,
                    into: Some(carrier),
                } = gate.operation
                else {
                    panic!("{name}: expected a staged carrier");
                };
                assert!(root(&from, b"gate: Option<bool>") && root(&carrier, b"gate"));
                assert!(
                    matches!(split.operation, Operation::TestVariant { operand, success }
                    if root(&operand, b"gate") && operand.root.node == carrier.root.node && success == &role.some.id)
                );
                let Terminator::Branch { yes, no } = split.terminator else {
                    panic!("expected both variants")
                };
                let effects = |edge| {
                    linear(mir, edge)
                        .into_iter()
                        .filter(|op| !matches!(op, Operation::None))
                        .collect::<Vec<_>>()
                };
                let failure = effects(no);
                assert!(
                    matches!(failure.as_slice(), [
                    Operation::ConstructVariant { variant, payload: None, into }, Operation::Export(export),
                    Operation::Drop(first), Operation::Drop(second), Operation::Cleanup(locals), Operation::Cleanup(inputs),
                ] if *variant == &role.none.id && root(into, b"gate?") && root(export, b"gate?") && into.root.node == export.root.node
                    && field(first, "first") && field(second, "second")
                    && matches!(locals.as_slice(), [Obligation::Drop(kept)] if root(kept, b"kept"))
                    && matches!(inputs.as_slice(), [Obligation::Drop(b), Obligation::EndStorage(gate)] if root(b, b"b: string") && root(gate, b"gate: Option<bool>"))),
                    "{name} None: {failure:?}"
                );

                let success = effects(yes);
                let [
                    Operation::CopyOptionPayload {
                        operand,
                        into: check,
                    },
                    Operation::Move {
                        from: b,
                        into: Some(last),
                    },
                    Operation::Initialize(packed),
                    Operation::Initialize(payload),
                    Operation::ConstructVariant {
                        variant,
                        payload: Some(carried),
                        into,
                    },
                    Operation::Export(export),
                    Operation::Cleanup(locals),
                    Operation::Cleanup(inputs),
                ] = success.as_slice()
                else {
                    panic!("{name} Some: {success:?}")
                };
                assert!(
                    root(operand, b"gate")
                        && operand.root.node == carrier.root.node
                        && field(check, "check")
                );
                assert!(root(b, b"b: string") && field(last, "last") && root(packed, b"packed"));
                assert!(
                    root(payload, b"true")
                        && root(carried, b"true")
                        && carried.root.node == payload.root.node
                );
                assert_eq!(*variant, &role.some.id);
                assert!(
                    root(into, b"Option.Some(true)")
                        && root(export, b"Option.Some(true)")
                        && into.root.node == export.root.node
                );
                assert!(
                    matches!(inputs.as_slice(), [Obligation::EndStorage(gate)] if root(gate, b"gate: Option<bool>"))
                );
                if record == "Custom" {
                    assert!(
                        matches!(locals.as_slice(), [Obligation::InvokeDisposer { operand, implementation }, Obligation::Drop(kept)]
                        if root(operand, b"packed") && operand.root.node == packed.root.node && *implementation == selected && root(kept, b"kept"))
                    );
                    let active = mir.disposer(selected).unwrap();
                    assert!(
                        matches!(path_drops(active, active.entry).as_slice(), [Obligation::Drop(last), Obligation::Drop(second), Obligation::Drop(first)]
                        if member(last, b"{}", "last") && member(second, b"{}", "second") && member(first, b"{}", "first"))
                    );
                } else {
                    assert!(
                        matches!(locals.as_slice(), [Obligation::Drop(last), Obligation::Drop(second), Obligation::EndStorage(check), Obligation::Drop(first), Obligation::EndStorage(owner), Obligation::Drop(kept)]
                        if field(last, "last") && field(second, "second") && field(check, "check") && field(first, "first")
                        && root(owner, b"packed") && owner.root.node == packed.root.node && root(kept, b"kept"))
                    );
                }

                let stats = report.statistics();
                let exact = AnalysisLimits {
                    max_work: stats.work,
                    max_entries: stats.entries,
                    max_depth: stats.max_depth,
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
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }

            let bad = lower("movedPrefix", AnalysisLimits::default());
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected only the moved prefix finding")
            };
            assert_eq!(finding.code(), "R0200");
            let offset = |bytes: &[u8]| {
                module
                    .syntax
                    .nodes
                    .iter()
                    .find(|node| node.span.bytes() == bytes)
                    .unwrap()
                    .span
                    .start()
            };
            for (site, start) in [
                (
                    finding.related,
                    offset(b"let prior = a;") + b"let prior = ".len(),
                ),
                (
                    finding.site,
                    offset(b"Pack { first: a, second: c, check: gate?, last: b }")
                        + b"Pack { first: ".len(),
                ),
            ] {
                assert_eq!(site.module, &module.key);
                assert_eq!(site.span.bytes(), b"a");
                assert_eq!(site.span.start(), start);
            }
        },
    );
}

#[test]
fn interrupted_record_initialization_keeps_unreached_and_unsupported_shapes_separate() {
    probe_with_typed(
        "records::interrupted_record_initialization_keeps_unreached_and_unsupported_shapes_separate",
        r#"
            type Pack = { first: string; check: bool; last: string; }
            type Pair = { check: bool; again: bool; }
            type Deeper = { inner: Pack; check: bool; }
            type Protected = { held: LockGuard<i32>; check: bool; }
            type Number = { data: i32; }
            type Text = { data: string; }
            type Ref = { view: &bool; check: bool; }
            function relay(flag: bool): bool { return flag; }
            function absent(): Option<bool>.None { return Option<bool>.None; }
            function badSuffix(b: string): string { let taken = b; return b; }
            function skipSuffix(a: string, b: string): Option<bool> {
                let packed = Pack { first: a, check: absent()?, last: badSuffix(b) };
                return Option.Some(true);
            }
            function reachSuffix(a: string, b: string): Option<bool> {
                let packed = Pack { first: a, check: Option<bool>.Some(true)?, last: badSuffix(b) };
                return Option.Some(true);
            }
            function certainNone(a: string, b: string): Option<bool> {
                let kept = b;
                let packed = Pack { first: a, check: Option<bool>.None?, last: b };
                return Option.Some(true);
            }
            function certainSome(a: string, b: string): Option<bool> {
                let packed = (Pack { first: a, check: Option<bool>.Some(true)?, last: b });
                return Option.Some(packed.check);
            }
            function embedded(gate: Option<bool>, a: string, b: string): Option<bool> {
                let packed = Pack { first: a, check: relay(gate?), last: b };
                return Option.Some(true);
            }
            function second(gate: Option<bool>): Option<bool> {
                let packed = Pair { check: Option<bool>.None?, again: gate? };
                return Option.Some(true);
            }
            function hiddenSuffix(gate: Option<bool>): Option<bool> {
                let packed = Pair { check: Option<bool>.None?, again: relay(gate?) };
                return Option.Some(true);
            }
            function textCarrier(value: Option<string>): Option<bool> {
                let packed = Text { data: value? };
                return Option.Some(true);
            }
            function reference(view: &bool, gate: Option<bool>): Option<bool> {
                let packed = Ref { view: view, check: gate? };
                return Option.Some(true);
            }
            function deeper(inner: Pack, gate: Option<bool>): Option<bool> {
                let packed = Deeper { inner: inner, check: gate? };
                return Option.Some(true);
            }
            function protected(held: LockGuard<i32>, gate: Option<bool>): Option<bool> {
                let packed = Protected { held: held, check: gate? };
                return Option.Some(true);
            }
            function rebuild(base: Pack, gate: Option<bool>): Option<bool> {
                let packed = Pack { ...base, check: gate? };
                return Option.Some(true);
            }
            function unnamed(a: string, gate: Option<bool>, b: string): Option<bool> {
                let _ = Pack { first: a, check: gate?, last: b };
                return Option.Some(true);
            }
            function wider(): Option<bool> {
                let packed = Number { data: Option<i32>.Some(1i32)? };
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let role = typed.option_role().unwrap();
            let skipped = lower("skipSuffix");
            let mir = skipped
                .mir()
                .expect("a known None factory skips the invalid suffix body");
            let calls: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter_map(|op| match op {
                    Operation::Call(callee) => Some(*callee),
                    _ => None,
                })
                .collect();
            assert!(
                matches!(calls.as_slice(), [id] if module.declarations.iter().any(|decl| &decl.id == *id && decl.name.as_deref() == Some("absent")))
            );
            let reached = lower("reachSuffix");
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("expected the reached suffix diagnostic")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"b");
            assert!(finding.related.span.start() < finding.site.span.start());
            for (name, variant, drop_names) in [
                ("certainNone", &role.none.id, ["first", "kept"]),
                ("certainSome", &role.some.id, ["last", "first"]),
            ] {
                let report = lower(name);
                let mir = report.mir().unwrap_or_else(|| {
                    panic!(
                        "{name}: {:?} at {:?}",
                        report.completion(),
                        report.blocked_at().map(|site| site
                            .span
                            .bytes()
                            .escape_ascii()
                            .to_string())
                    )
                });
                let path = linear(mir, mir.entry);
                let exports: Vec<_> = path
                    .iter()
                    .filter_map(|op| match op {
                        Operation::Export(place) => Some(place),
                        _ => None,
                    })
                    .collect();
                let [export] = exports.as_slice() else {
                    panic!("{name}: expected one export");
                };
                if name == "certainNone" {
                    let export_at = path
                        .iter()
                        .position(|op| matches!(op, Operation::Export(_)))
                        .unwrap();
                    assert!(
                        !path[..export_at]
                            .iter()
                            .any(|op| matches!(op, Operation::Drop(_) | Operation::Cleanup(_))),
                        "failure must export before prefix or lexical cleanup"
                    );
                }
                assert!(path.iter().any(|op| matches!(op, Operation::ConstructVariant { variant: made, into, .. }
                    if *made == variant && into.root.node == export.root.node && into.projection.is_none())));
                assert_eq!(path.iter().any(|op| matches!(op, Operation::Initialize(place) if place.root.span.bytes() == b"packed")), name == "certainSome");
                let drops = path_drops(mir, mir.entry);
                let [Obligation::Drop(first), Obligation::Drop(second)] = drops.as_slice() else {
                    panic!("{name}: expected two drops");
                };
                for (place, name) in [(first, drop_names[0]), (second, drop_names[1])] {
                    if name == "kept" {
                        assert_eq!(place.root.span.bytes(), b"kept");
                        assert!(place.projection.is_none());
                    } else {
                        assert_eq!(place.root.span.bytes(), b"packed");
                        assert_eq!(
                            place.field().map(|field| &field.id),
                            Some(canonical_record_field(typed, &module.key, "Pack", name))
                        );
                    }
                }
            }
            for (name, blocked) in [
                (
                    "embedded",
                    b"Pack { first: a, check: relay(gate?), last: b }".as_slice(),
                ),
                (
                    "second",
                    b"Pair { check: Option<bool>.None?, again: gate? }",
                ),
                (
                    "hiddenSuffix",
                    b"Pair { check: Option<bool>.None?, again: relay(gate?) }",
                ),
                ("textCarrier", b"Text { data: value? }"),
                ("reference", b"Ref { view: view, check: gate? }"),
                ("deeper", b"Deeper { inner: inner, check: gate? }"),
                ("protected", b"Protected { held: held, check: gate? }"),
                ("rebuild", b"Pack { ...base, check: gate? }"),
                ("unnamed", b"Pack { first: a, check: gate?, last: b }"),
                ("wider", b"Option<i32>.Some(1i32)?"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(
                    report.mir().is_none() && report.findings().is_empty(),
                    "{name}"
                );
                assert_eq!(report.blocked_at().unwrap().span.bytes(), blocked, "{name}");
            }
        },
    );
}

#[test]
fn named_flat_rebuild_stages_before_displacement_and_transfers_cleanup() {
    probe_with_typed(
        "records::named_flat_rebuild_stages_before_displacement_and_transfers_cleanup",
        r#"
            type Pack = { keep: string; mark: bool; last: string; }
            function rebuilt(base: Pack, replacement: string): void {
                let saved = Pack { ...base, last: replacement, mark: base.mark };
            }
        "#,
        |_, module, receipt, typed| {
            let declaration = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("rebuilt"))
                .unwrap();
            let lower = |limits| lower_body(receipt, module, &declaration.id, limits).unwrap();
            let default = AnalysisLimits::default();
            let report = lower(default);
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "rebuild: {:?} at {:?}",
                    report.completion(),
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                )
            });
            let field = |name| canonical_record_field(typed, &module.key, "Pack", name);
            let ops: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    !matches!(op, Operation::None)
                        && !matches!(op, Operation::Cleanup(actions) if actions.is_empty())
                })
                .collect();
            let [
                Operation::Move {
                    from: replacement,
                    into: Some(last_stage),
                },
                Operation::Copy {
                    from: original_mark,
                    into: Some(mark_stage),
                },
                Operation::Drop(displaced_last),
                Operation::Install {
                    target: last,
                    incoming: new_last,
                },
                Operation::Install {
                    target: mark,
                    incoming: new_mark,
                },
                Operation::Move {
                    from: base,
                    into: Some(saved),
                },
                Operation::Cleanup(exit),
            ] = ops.as_slice()
            else {
                panic!("expected only stages, replacements, handoff and exit")
            };
            assert_eq!(replacement.root.span.bytes(), b"replacement: string");
            assert_eq!(base.root.span.bytes(), b"base: Pack");
            assert_eq!(saved.root.span.bytes(), b"saved");
            for whole in [
                replacement,
                last_stage,
                mark_stage,
                new_last,
                new_mark,
                base,
                saved,
            ] {
                assert!(whole.projection.is_none());
            }
            assert_eq!(last_stage.root.node, new_last.root.node);
            assert_eq!(mark_stage.root.node, new_mark.root.node);
            assert_ne!(last_stage.root.node, mark_stage.root.node);
            for (place, name) in [(original_mark, "mark"), (mark, "mark"), (last, "last")] {
                assert_eq!(place.root.node, base.root.node);
                assert_eq!(&place.field().unwrap().id, field(name));
            }
            assert_eq!(displaced_last.root.node, base.root.node);
            assert_eq!(&displaced_last.field().unwrap().id, field("last"));
            assert!(
                matches!(exit.as_slice(), [Obligation::Drop(last), Obligation::EndStorage(mark), Obligation::Drop(keep), Obligation::EndStorage(whole)]
                if whole.root.node == saved.root.node && whole.projection.is_none() && [last, mark, keep].iter().zip(["last", "mark", "keep"]).all(|(place, name)| place.root.node == saved.root.node && &place.field().unwrap().id == field(name)))
            );

            let exact = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                max_depth: report.statistics().max_depth,
            };
            assert!(lower(exact).mir().is_some());
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
                let limited = lower(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn flat_rebuild_distinguishes_moved_inputs_live_loans_and_unsupported_bases() {
    probe(
        "records::flat_rebuild_distinguishes_moved_inputs_live_loans_and_unsupported_bases",
        r#"
            type Pack = { keep: string; mark: bool; last: string; }
            type Flags = { mark: bool; }
            type Outer = { child: Pack; }
            function reused(base: Pack, replacement: string): string {
                let saved = Pack { ...base, last: replacement, mark: base.mark };
                return base.keep;
            }
            function movedBase(base: Pack, replacement: string): void {
                let consumed = base;
                let saved = Pack { ...base, last: replacement };
            }
            function movedReplacement(base: Pack, replacement: string): void {
                let consumed = replacement;
                let saved = Pack { ...base, last: replacement };
            }
            function movedField(base: Pack): void {
                let consumed = base.last;
                let saved = Pack { ...base, last: base.last };
            }
            function moveOriginal(base: Pack): void {
                let saved = Pack { ...base, last: base.last };
            }
            function movedFieldGrouped(base: Pack): void {
                let consumed = base.last;
                let saved = Pack { ...base, last: ((base.last)) };
            }
            function conflict(base: Pack, replacement: string): &string {
                let view = &base.keep;
                let saved = Pack { ...base, last: replacement };
                return view;
            }
            function repair(base: Pack, replacement: string): void {
                let hole = base.last;
                let saved = Pack { ...base, last: replacement };
            }
            function pass(base: Pack): Pack { return base; }
            function computed(base: Pack, replacement: string): void {
                let saved = Pack { ...pass(base), last: replacement };
            }
            function nested(base: Outer, replacement: string): void {
                let saved = Pack { ...base.child, last: replacement };
            }
            function copyOnly(base: Flags): void {
                let saved = Flags { ...base, mark: true };
            }
            function direct(base: Pack, replacement: string): Pack {
                return Pack { ...base, last: replacement };
            }
            function interrupted(base: Pack, replacement: string, gate: Option<bool>): Option<bool> {
                let saved = Pack { ...base, last: replacement, mark: gate? };
                return gate;
            }
        "#,
        |_, module, receipt| {
            let check = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let lower = |name| check(name, AnalysisLimits::default());
            let bare = lower("movedField");
            let depth = AnalysisLimits {
                max_depth: bare.statistics().max_depth,
                ..AnalysisLimits::default()
            };
            assert_eq!(
                check("movedField", depth).completion(),
                Completion::SourceRejected
            );
            let limited = check("movedFieldGrouped", depth);
            assert_eq!(limited.completion(), Completion::ResourceLimit);
            assert!(limited.mir().is_none() && limited.findings().is_empty());
            for (name, code, site, related) in [
                (
                    "reused",
                    "R0200",
                    "base.keep",
                    "Pack { ...base, last: replacement, mark: base.mark }",
                ),
                ("movedBase", "R0200", "base", "base"),
                ("movedReplacement", "R0200", "replacement", "replacement"),
                ("movedField", "R0200", "base.last", "base.last"),
                ("movedFieldGrouped", "R0200", "base.last", "base.last"),
                (
                    "conflict",
                    "R0203",
                    "Pack { ...base, last: replacement }",
                    "&base.keep",
                ),
            ] {
                let report = lower(name);
                assert_eq!(
                    report.completion(),
                    Completion::SourceRejected,
                    "{name}: {:?}",
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected one finding for {name}")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
                assert!(finding.related.span.end() <= finding.site.span.start());
            }
            for (name, site) in [
                ("repair", "Pack { ...base, last: replacement }"),
                ("moveOriginal", "base.last"),
                ("computed", "Pack { ...pass(base), last: replacement }"),
                ("nested", "Pack { ...base.child, last: replacement }"),
                ("copyOnly", "Flags { ...base, mark: true }"),
                ("direct", "Pack { ...base, last: replacement }"),
                (
                    "interrupted",
                    "Pack { ...base, last: replacement, mark: gate? }",
                ),
            ] {
                let report = lower(name);
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

fn dropped_fields<'g>(mir: &Mir<'g>) -> Vec<&'g str> {
    path_drops(mir, mir.entry)
        .into_iter()
        .filter_map(|drop| match drop {
            Obligation::Drop(place) => place.field().and_then(|field| field.name.as_deref()),
            _ => None,
        })
        .collect()
}
