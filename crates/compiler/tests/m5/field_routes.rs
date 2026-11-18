use super::*;

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
            function borrowHolder(): void {
                let mut local = true;
                let x = Holder { value: &local };
                let access = &x;
                local = false;
                let last = access;
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
            let mir = forwarded
                .mir()
                .unwrap_or_else(|| panic!("{:?}", forwarded.completion()));
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Copy { from, into: Some(_) }
                if from.root.span.bytes() == b"x: Holder" && from.field.is_some_and(|field| field.name.as_deref() == Some("value")))));
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Export(place) if place.root.span.bytes() == b"x.value")));
            assert!(mir.blocks.iter().any(|block| matches!(&block.operation, Operation::Cleanup(cleanup)
                if matches!(cleanup.as_slice(), [Obligation::EndStorage(field), Obligation::EndStorage(root)]
                    if field.field.is_some_and(|decl| decl.name.as_deref() == Some("value"))
                        && root.field.is_none() && root.root.span.bytes() == b"x: Holder"))));
            assert!(path_drops(mir, mir.entry).is_empty());

            let mixed = lower("forwardAndClean");
            let mir = mixed
                .mir()
                .unwrap_or_else(|| panic!("{:?}", mixed.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.root.span.bytes() == b"x: WithText" && place.field.is_some_and(|decl| decl.name.as_deref() == Some("text")))
            );

            let caller = lower("callerForward");
            let mir = caller
                .mir()
                .unwrap_or_else(|| panic!("{:?}", caller.completion()));
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
            let mir = replacement
                .mir()
                .unwrap_or_else(|| panic!("{:?}", replacement.completion()));
            assert!(mir.blocks.iter().any(
                |block| matches!(block.operation, Operation::Install { target, .. }
                if target.field.is_some_and(|decl| decl.name.as_deref() == Some("value")))
            ));
            let ordered = lower("storageBeforeSource");
            let mir = ordered
                .mir()
                .unwrap_or_else(|| panic!("{:?}", ordered.completion()));
            assert!(mir.blocks.iter().any(|block| matches!(&block.operation, Operation::Cleanup(cleanup)
                if matches!(cleanup.as_slice(), [Obligation::EndStorage(field), Obligation::EndStorage(root), Obligation::EndStorage(source)]
                    if field.field.is_some_and(|decl| decl.name.as_deref() == Some("value"))
                        && root.field.is_none() && root.root.span.bytes() == b"x"
                        && source.root.span.bytes() == b"local"))));

            let looped = lower("loopClears");
            let mir = looped
                .mir()
                .unwrap_or_else(|| panic!("{:?}", looped.completion()));
            assert!(
                mir.blocks
                    .iter()
                    .enumerate()
                    .any(|(pc, b)| matches!(b.terminator, Terminator::Goto(next) if next < pc))
            );

            let grouped = lower("groupedOnly");
            assert_eq!(grouped.completion(), Completion::Incomplete);
            assert!(grouped.mir().is_none());

            for name in [
                "callerRetains",
                "otherHolder",
                "possibleSource",
                "futureProjection",
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
