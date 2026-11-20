use super::*;
use compiler::analysis::ownership::Report;

fn require_mir<'a, 'g>(report: &'a Report<'g>, name: &str) -> &'a Mir<'g> {
    report
        .mir()
        .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()))
}

#[test]
fn carried_field_and_slot_borrow_differ() {
    probe(
        "field_routes::carried_field_and_slot_borrow_differ",
        r#"
            type Holder = { value: &bool; }
            type WithText = { text: string; value: &bool; }
            function forward(x: Holder): &bool { return x.value; }
            function escapes(x: Holder): &&bool { return &x.value; }
            function forwardAndClean(x: WithText): &bool { return x.value; }
            function escapeLocal(): &bool {
                let local = true;
                let x = Holder { value: &local };
                return x.value;
            }
            function escapeInRecord(): Holder {
                let local = true;
                return Holder { value: &local };
            }
            function callerForward(seed: &bool, text: string): &bool {
                let x = WithText { text: text, value: seed };
                return forwardAndClean(x);
            }
            function callerEscapes(): &bool {
                let local = true;
                let x = Holder { value: &local };
                return forward(x);
            }
            function callerRetains(): bool {
                let mut local = true;
                let x = Holder { value: &local };
                let view = forward(x);
                local = false;
                return *view;
            }
            function fieldReplacement(seed: &bool): &bool {
                let mut local = true;
                let mut x = Holder { value: &local };
                x.value = seed;
                local = false;
                return x.value;
            }
            function otherHolder(seed: &bool): bool {
                let mut local = true;
                let mut x = Holder { value: &local };
                let other = x;
                x.value = seed;
                local = false;
                return *other.value;
            }
            function storageBeforeSource(seed: &bool): void {
                let mut x = Holder { value: seed };
                let local = true;
                x.value = &local;
            }
            type Two = { first: &bool; second: &bool; }
            function possibleSource(seed: &bool): bool {
                let mut local = true;
                let mut x = Two { first: &local, second: seed };
                let carried = x.second;
                x.first = seed;
                local = false;
                return *carried;
            }
            function futureProjection(seed: &bool): &bool {
                let mut local = true;
                let x = Two { first: &local, second: seed };
                local = false;
                return x.second;
            }
            function reborrowEscape(seed: &bool): &bool {
                let local = true;
                let x = Two { first: &local, second: seed };
                return &*x.second;
            }
            function reborrowAfterRelease(seed: &bool): &bool {
                let mut local = true;
                let x = Two { first: &local, second: seed };
                local = false;
                return &*x.second;
            }
            function groupedReborrow(x: Holder): &bool { return &(*x.value); }
            function borrowHolder(): void {
                let mut local = true;
                let x = Holder { value: &local };
                let access = &x;
                local = false;
                let last = access;
            }
            function freeze(source: &mut bool): &bool { return &*source; }
            function retainsExclusiveRestriction(): bool {
                let mut local = true;
                let view = freeze(&mut local);
                let x = Holder { value: view };
                let holder = x;
                let before = local;
                return *holder.value;
            }
            function branchRetains(flag: bool, seed: &bool): bool {
                let mut local = true;
                let mut x = Holder { value: seed };
                if (flag) { x.value = &local; }
                local = false;
                return *x.value;
            }
            function loopEscapes(flag: bool, seed: &bool): &bool {
                let mut x = Holder { value: seed };
                while (flag) {
                    let local = true;
                    x.value = &local;
                    break;
                }
                return x.value;
            }
            function loopClears(flag: bool, seed: &bool): &bool {
                let mut x = Holder { value: seed };
                while (flag) {
                    let local = true;
                    x.value = &local;
                    let used = *x.value;
                    x.value = seed;
                    continue;
                }
                return x.value;
            }
            type LocalView = { owned: bool; view: &bool; }
            function groupedOnly(seed: &bool): void {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
            }
            type ReverseView = { view: &bool; owned: bool; }
            function outOfOrder(seed: &bool): void {
                let mut x = ReverseView { owned: true, view: seed };
                x.view = &x.owned;
            }
            function recordCycle(seed: &bool): void {
                let mut x = LocalView { owned: true, view: seed };
                let mut y = LocalView { owned: false, view: seed };
                x.view = &y.owned;
                y.view = &x.owned;
            }
            type TextView = { owned: bool; view: &bool; text: string; }
            function observable(seed: &bool, text: string): void {
                let mut x = TextView { owned: true, view: seed, text: text };
                x.view = &x.owned;
            }
            function moveOwner(seed: &bool, text: string): bool {
                let mut x = TextView { owned: true, view: seed, text: text };
                x.view = &x.owned;
                let moved = x;
                return *moved.view;
            }
            type GuardView = { owned: bool; view: &bool; lock: LockGuard<i32>; }
            function protected(seed: &bool, lock: LockGuard<i32>): void {
                let mut x = GuardView { owned: true, view: seed, lock: lock };
                x.view = &x.owned;
            }
            function replaceOwner(seed: &bool): void {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
                x = LocalView { owned: false, view: seed };
            }
            function usedCopy(seed: &bool): bool {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
                let copied = x.view;
                x.view = seed;
                let observed = *copied;
                x.owned = false;
                return observed;
            }
            function liveCopy(seed: &bool): bool {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
                let copied = x.view;
                x.view = seed;
                x.owned = false;
                return *copied;
            }
            function liveAcrossBoundary(seed: &bool): bool {
                let mut copied = seed;
                {
                    let mut x = LocalView { owned: true, view: seed };
                    x.view = &x.owned;
                    copied = x.view;
                }
                return *copied;
            }
        "#,
        |_, module, receipt| {
            let lower_with = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let lower = |name| lower_with(name, AnalysisLimits::default());

            let forwarded = lower("forward");

            let mir = require_mir(&forwarded, "forward");
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Copy { from, into: Some(_) }
                if from.root.span.bytes() == b"x: Holder" && from.field().is_some_and(|field| field.name.as_deref() == Some("value")))));
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Export(place) if place.root.span.bytes() == b"x.value")));
            assert!(mir.blocks.iter().any(|block| matches!(&block.operation, Operation::Cleanup(cleanup)
                if matches!(cleanup.as_slice(), [Obligation::EndStorage(field), Obligation::EndStorage(root)]
                    if field.field().is_some_and(|decl| decl.name.as_deref() == Some("value"))
                        && root.projection.is_none() && root.root.span.bytes() == b"x: Holder"))));
            assert!(path_drops(mir, mir.entry).is_empty());

            let mixed = lower("forwardAndClean");

            let mir = require_mir(&mixed, "forwardAndClean");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.root.span.bytes() == b"x: WithText" && place.field().is_some_and(|decl| decl.name.as_deref() == Some("text")))
            );

            let caller = lower("callerForward");

            let mir = require_mir(&caller, "callerForward");
            assert!(
                mir.blocks
                    .iter()
                    .any(|block| matches!(block.operation, Operation::Call(_)))
            );
            assert!(
                path_drops(mir, mir.entry).is_empty(),
                "the call owns the transferred text"
            );
            let replacement = lower("fieldReplacement");

            let mir = require_mir(&replacement, "fieldReplacement");
            assert!(mir.blocks.iter().any(
                |block| matches!(block.operation, Operation::Install { target, .. }
                if target.field().is_some_and(|decl| decl.name.as_deref() == Some("value")))
            ));
            let ordered = lower("storageBeforeSource");

            let mir = require_mir(&ordered, "storageBeforeSource");
            assert!(mir.blocks.iter().any(|block| matches!(&block.operation, Operation::Cleanup(cleanup)
                if matches!(cleanup.as_slice(), [Obligation::EndStorage(field), Obligation::EndStorage(root), Obligation::EndStorage(source)]
                    if field.field().is_some_and(|decl| decl.name.as_deref() == Some("value"))
                        && root.projection.is_none() && root.root.span.bytes() == b"x"
                        && source.root.span.bytes() == b"local"))));

            let looped = lower("loopClears");

            let mir = require_mir(&looped, "loopClears");
            assert!(
                mir.blocks
                    .iter()
                    .enumerate()
                    .any(|(pc, b)| matches!(b.terminator, Terminator::Goto(next) if next < pc))
            );

            let grouped_reborrow = lower("groupedReborrow");
            let mir = require_mir(&grouped_reborrow, "groupedReborrow");
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Reborrow { parent, exclusive: false, .. }
                if parent.field().is_some_and(|field| field.name.as_deref() == Some("value")))));

            let exclusive = lower("retainsExclusiveRestriction");
            assert_eq!(exclusive.completion(), Completion::SourceRejected);
            assert!(exclusive.mir().is_none());
            let [finding] = exclusive.findings() else {
                panic!("expected exclusive restriction")
            };
            assert_eq!(finding.code(), "R0201");
            assert_eq!(finding.related.span.bytes(), b"&mut local");

            let grouped = lower("groupedOnly");
            let mir = require_mir(&grouped, "groupedOnly");
            assert_eq!(grouped.completion(), Completion::Checked);
            assert!(grouped.findings().is_empty());
            let ops = linear(mir, mir.entry);
            let (backing, staged) = ops
                .iter()
                .find_map(|op| match op {
                    Operation::Borrow {
                        source,
                        holder,
                        exclusive: false,
                    } => Some((source, holder)),
                    _ => None,
                })
                .expect("borrow the sibling field");
            let stored = ops
                .iter()
                .find_map(|op| match op {
                    Operation::Install { target, incoming }
                        if incoming.root.node == staged.root.node =>
                    {
                        Some(target)
                    }
                    _ => None,
                })
                .expect("install the borrowed view");
            let owned = &backing.field().unwrap().id;
            let view = &stored.field().unwrap().id;
            assert_eq!(backing.field().unwrap().name.as_deref(), Some("owned"));
            assert_eq!(stored.field().unwrap().name.as_deref(), Some("view"));
            let x = backing.root.node;
            assert_eq!(backing.root.span.bytes(), b"x");
            assert_eq!(stored.root.node, x);
            let mut ends = Vec::new();
            for block in straight_blocks(mir, mir.entry) {
                let Operation::Cleanup(actions) = &block.operation else {
                    continue;
                };
                for action in actions {
                    if let Obligation::EndStorage(place) = action
                        && place.root.node == x
                    {
                        assert_eq!(block.site.span.bytes(), b"{\n                let mut x = LocalView { owned: true, view: seed };\n                x.view = &x.owned;\n            }");
                        assert!(place.projection.is_none() || place.field().is_some());
                        ends.push(place.field().map(|field| &field.id));
                    }
                }
            }
            assert_eq!(ends, [Some(view), Some(owned), None]);
            assert!(path_drops(mir, mir.entry).is_empty());

            let limits = AnalysisLimits {
                max_work: grouped.statistics().work,
                max_entries: grouped.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower_with("groupedOnly", limits).mir().is_some());
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
                let limited = lower_with("groupedOnly", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

            for name in [
                "outOfOrder",
                "recordCycle",
                "observable",
                "protected",
                "replaceOwner",
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.findings().is_empty(), "{name}");
                assert!(report.mir().is_none(), "{name}");
                let site = report.blocked_at().unwrap().span.bytes();
                if name == "replaceOwner" {
                    assert_eq!(site, b"x = LocalView { owned: false, view: seed };");
                } else {
                    assert!(
                        site.starts_with(b"{\n                let mut x = "),
                        "{name}: {site:?}"
                    );
                }
            }
            let moved = lower("moveOwner");
            assert_eq!(moved.completion(), Completion::SourceRejected);
            assert!(moved.mir().is_none());
            let [finding] = moved.findings() else {
                panic!("expected the self-borrow to block the owner move")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"x");
            assert_eq!(finding.related.span.bytes(), b"&x.owned");

            let used = lower("usedCopy");
            let mir = require_mir(&used, "usedCopy");
            let ops = linear(mir, mir.entry);
            let staged = ops
                .iter()
                .find_map(|op| match op {
                    Operation::Borrow {
                        source,
                        holder,
                        exclusive: false,
                    } if source.root.span.bytes() == b"x"
                        && source
                            .field()
                            .is_some_and(|f| f.name.as_deref() == Some("owned")) =>
                    {
                        Some(holder)
                    }
                    _ => None,
                })
                .expect("borrow the owned field, not the whole record");
            assert!(
                ops.iter()
                    .any(|op| matches!(op, Operation::Install { target, incoming }
                if target.root.span.bytes() == b"x"
                    && target.field().is_some_and(|f| f.name.as_deref() == Some("view"))
                    && incoming.root.node == staged.root.node))
            );
            assert!(
                ops.iter()
                    .any(|op| matches!(op, Operation::Copy { from, into: Some(to) }
                if from.root.span.bytes() == b"x"
                    && from.field().is_some_and(|f| f.name.as_deref() == Some("view"))
                    && to.root.span.bytes() == b"copied"))
            );

            let last_use = ops
                .iter()
                .position(|op| {
                    matches!(op, Operation::Dereference(place)
                if place.root.span.bytes() == b"copied")
                })
                .expect("use the copied view");
            let replace = ops
                .iter()
                .position(|op| {
                    matches!(op, Operation::Install { target, .. }
                if target.field().is_some_and(|field| &field.id == owned))
                })
                .expect("replace the backing field");
            assert!(last_use < replace);

            for (name, site) in [
                ("liveCopy", b"x.owned = false;".as_slice()),
                ("liveAcrossBoundary", b"{\n                    let mut x = LocalView { owned: true, view: seed };\n                    x.view = &x.owned;\n                    copied = x.view;\n                }"),
            ] {
                let live = lower(name);
                assert_eq!(live.completion(), Completion::SourceRejected, "{name}");
                assert!(live.mir().is_none(), "{name}");
                let [finding] = live.findings() else {
                    panic!("{name}: expected the copied view to block release of its backing");
                };
                assert_eq!(finding.code(), "R0203", "{name}");
                assert_eq!(finding.site.span.bytes(), site, "{name}");
                assert_eq!(finding.related.span.bytes(), b"&x.owned", "{name}");
            }

            for name in [
                "callerRetains",
                "otherHolder",
                "possibleSource",
                "futureProjection",
                "reborrowAfterRelease",
                "borrowHolder",
                "branchRetains",
                "loopEscapes",
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none(), "{name}");
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected release finding")
                };
                assert_eq!(finding.code(), "R0203", "{name}");
                assert_eq!(finding.related.span.bytes(), b"&local", "{name}");
            }

            for (name, source) in [
                ("escapes", b"&x.value".as_slice()),
                ("escapeLocal", b"local".as_slice()),
                ("escapeInRecord", b"local".as_slice()),
                ("callerEscapes", b"local".as_slice()),
                ("reborrowEscape", b"local".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none(), "{name}");
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected an escape finding")
                };
                assert_eq!(finding.code(), "R0202", "{name}");
                assert_eq!(finding.related.span.bytes(), source, "{name}");
            }
        },
    );
}
