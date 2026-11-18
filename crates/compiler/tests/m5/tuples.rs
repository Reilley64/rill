use super::*;
use compiler::analysis::ownership::Projection;

#[test]
fn fixed_tuple_places_preserve_initialization_and_order() {
    probe(
        "tuples::fixed_tuple_places_preserve_initialization_and_order",
        r#"
            function split(flag: bool, value: [string, string, string]): string {
                if (flag) { let taken = value[0]; }
                return value[1];
            }
            function usedAfterJoin(flag: bool, value: [string, string]): string {
                if (flag) { let taken = value[0]; }
                return value[0];
            }
            function wholeAfterJoin(flag: bool, value: [string, string]): [string, string] {
                if (flag) { let taken = value[0]; }
                return value;
            }
            function construct(first: string, second: string): void {
                let pair: [string, string] = [first, second];
            }
            function restore(seed: [string, string]): string {
                let mut pair = seed;
                let taken = pair[0];
                pair[0] = "new";
                return taken;
            }
            function replace(seed: [string, string], new: string): void {
                let mut pair = seed;
                pair[0] = new;
            }
            function forward(value: [&bool, string]): &bool { return value[0]; }
            function forwardCall(value: [&bool, string]): &bool { return forward(value); }
            function borrowElement(value: [bool, bool]): bool {
                let r = &value[0];
                return *r;
            }
            function pass(value: [string, string, string]): [string, string, string] { return value; }
            function temporary(value: [string, string, string]): string { return pass(value)[1]; }
        "#,
        |_, module, receipt| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };

            let split = lower("split");
            let mir = split
                .mir()
                .unwrap_or_else(|| panic!("split: {:?}", split.completion()));
            let (yes, no) = branch_targets(mir);
            let cleanup = path_drops(mir, yes);
            assert!(
                matches!(cleanup.as_slice(), [Obligation::Drop(taken), Obligation::Drop(sibling)]
                if taken.root.span.bytes() == b"taken" && sibling.tuple_element() == Some(2))
            );
            assert_eq!(dropped_elements(mir, no), [2, 0]);

            for (name, use_site) in [
                ("usedAfterJoin", b"value[0]".as_slice()),
                ("wholeAfterJoin", b"value".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected moved element")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), use_site);
                assert_eq!(finding.related.span.bytes(), b"value[0]");
            }

            let constructed = lower("construct");
            let mir = constructed.mir().expect("tuple construction MIR");
            let order: Vec<_> = mir
                .blocks
                .iter()
                .filter_map(|block| match block.operation {
                    Operation::Move {
                        from,
                        into: Some(into),
                    } => into.tuple_element().map(|i| (from.root.span.bytes(), i)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                order,
                [
                    (b"first: string".as_slice(), 0),
                    (b"second: string".as_slice(), 1)
                ]
            );
            assert_eq!(dropped_elements(mir, mir.entry), [1, 0]);
            let restored = lower("restore");
            let mir = restored.mir().expect("tuple replacement MIR");
            assert_eq!(dropped_elements(mir, mir.entry), [1, 0]);
            assert!(
                mir.blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::Install { target, .. }
                if target.tuple_element() == Some(0)))
            );

            let replaced = lower("replace");
            let mir = replaced.mir().expect("initialized element replacement MIR");
            assert_eq!(dropped_elements(mir, mir.entry), [0, 1, 0]);
            assert!(mir.blocks.iter().any(|b| matches!(b.terminator, Terminator::Goto(next)
                if matches!(b.operation, Operation::Drop(old) if old.tuple_element() == Some(0))
                    && matches!(mir.blocks[next].operation, Operation::Install { target, .. } if target.tuple_element() == Some(0)))));

            let forwarded = lower("forward");
            let mir = forwarded.mir().expect("carried tuple reference MIR");
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Copy { from, into: Some(_) }
                if matches!(from.projection, Some(Projection::TupleElement(0))))
            ));
            assert_eq!(dropped_elements(mir, mir.entry), [1]);

            let forwarded = lower("forwardCall");
            let mir = forwarded.mir().expect("call with carried tuple input MIR");
            assert!(
                mir.blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::Call(_)))
            );

            let borrowed = lower("borrowElement");
            assert_eq!(borrowed.completion(), Completion::Incomplete);
            assert!(borrowed.mir().is_none());

            let temporary = lower("temporary");
            let mir = temporary.mir().expect("tuple result projection MIR");
            assert!(
                mir.blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::Call(_)))
            );
            assert_eq!(dropped_elements(mir, mir.entry), [2, 0]);
        },
    );
}

fn dropped_elements(mir: &Mir<'_>, start: usize) -> Vec<usize> {
    path_drops(mir, start)
        .into_iter()
        .map(|action| match action {
            Obligation::Drop(place) => place.tuple_element().expect("tuple cleanup position"),
            other => panic!("unexpected tuple cleanup {other:?}"),
        })
        .collect()
}
