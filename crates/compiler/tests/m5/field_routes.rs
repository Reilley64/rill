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
        "#,
        |_, module, receipt| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };

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
            assert_eq!(grouped.completion(), Completion::Incomplete);
            assert!(grouped.mir().is_none());

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
