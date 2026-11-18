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
            function forwardLocal(p: bool): &bool {
                let value: [&bool, string] = [&p, "tag"];
                return forward(value);
            }
            function borrowElement(value: [bool, bool]): bool {
                let r = &value[0];
                return *r;
            }
            function pass(value: [string, string, string]): [string, string, string] { return value; }
            function temporary(value: [string, string, string]): string { return pass(value)[1]; }
            function bindTemporary(value: [string, string, string]): string {
                let selected = pass(value)[0x1];
                return selected;
            }
            function assignTemporary(value: [string, string, string], old: string): string {
                let mut selected = old;
                selected = pass(value)[1];
                return selected;
            }
            function carry(p: &bool): [&bool, string] { return [p, "owned"]; }
            function carrier(p: &bool): &bool { return carry(p)[0]; }
            function carrierLocal(p: bool): &bool { return carry(&p)[0]; }
            function wholeLoan(value: [string, string]): string {
                let all = &value;
                let selected = value[1];
                let _ = all;
                return selected;
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
            let mut order = Vec::new();
            let mut pc = mir.entry;
            for _ in 0..mir.blocks.len() {
                let block = &mir.blocks[pc];
                if let Operation::Move {
                    from,
                    into: Some(into),
                } = block.operation
                    && let Some(position) = into.tuple_element()
                {
                    order.push((from.root.span.bytes(), position));
                }
                match block.terminator {
                    Terminator::Goto(next) => pc = next,
                    Terminator::Return => break,
                    other => panic!("expected a linear path, got {other:?}"),
                }
            }
            assert!(matches!(mir.blocks[pc].terminator, Terminator::Return));
            assert_eq!(
                order,
                [
                    (b"first: string".as_slice(), 0),
                    (b"second: string".as_slice(), 1)
                ]
            );
            assert_eq!(dropped_elements(mir, mir.entry), [1, 0]);
            let limits = AnalysisLimits {
                max_work: constructed.statistics().work,
                max_entries: constructed.statistics().entries,
                ..AnalysisLimits::default()
            };
            let check =
                |limits| lower_body(receipt, module, &declaration("construct").id, limits).unwrap();
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
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
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

            let local = lower("forwardLocal");
            assert_eq!(local.completion(), Completion::SourceRejected);
            assert!(local.mir().is_none());
            let [finding] = local.findings() else {
                panic!("expected local reference escape")
            };
            assert_eq!(finding.code(), "R0202");
            assert_eq!(finding.related.span.bytes(), b"p: bool");

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
            let output = mir
                .blocks
                .iter()
                .find_map(|b| match b.operation {
                    Operation::Export(place) => Some(place),
                    _ => None,
                })
                .expect("return the selected element");
            assert!(mir.blocks.iter().any(|b| matches!(b.operation, Operation::Move { from, into: Some(into) }
                if from.tuple_element() == Some(1) && into.root.node == output.root.node && into.projection.is_none())));

            let bound = lower("bindTemporary");
            let mir = bound
                .mir()
                .unwrap_or_else(|| panic!("bindTemporary: {:?}", bound.completion()));
            assert_eq!(dropped_elements(mir, mir.entry), [2, 0]);
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Move { from, into: Some(into) }
                if from.tuple_element() == Some(1) && into.root.span.bytes() == b"selected")
            ));

            let assigned = lower("assignTemporary");
            let mir = assigned.mir().expect("assignment from tuple temporary");
            assert!(matches!(path_drops(mir, mir.entry).as_slice(),
                [Obligation::Drop(old), Obligation::Drop(high), Obligation::Drop(low)]
                    if old.root.span.bytes() == b"selected" && high.tuple_element() == Some(2) && low.tuple_element() == Some(0)));
            assert!(mir.blocks.iter().any(|b| matches!(b.terminator, Terminator::Goto(next)
                if matches!(b.operation, Operation::Install { target, .. } if target.root.span.bytes() == b"selected")
                    && matches!(&mir.blocks[next].operation, Operation::Cleanup(actions)
                        if actions.iter().any(|a| matches!(a, Obligation::Drop(place) if place.tuple_element() == Some(2)))))));

            let carrier = lower("carrier");
            let mir = carrier
                .mir()
                .expect("caller backing survives aggregate call result");
            assert_eq!(dropped_elements(mir, mir.entry), [1]);
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Copy { from, into: Some(_) }
                if from.tuple_element() == Some(0))
            ));
            for (name, code, related) in [
                ("carrierLocal", "R0202", b"p: bool".as_slice()),
                ("wholeLoan", "R0203", b"&value".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected a loan diagnostic")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.related.span.bytes(), related);
            }
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
