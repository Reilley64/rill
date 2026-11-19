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

#[test]
fn fixed_tuple_loans_track_exact_members_without_proving_disjointness() {
    probe(
        "tuples::fixed_tuple_loans_track_exact_members_without_proving_disjointness",
        r#"
            function shared(value: [bool, bool, string]): bool {
                let first = &value[0];
                let second = &value[0x1];
                let _ = *first;
                return *second;
            }
            function afterMove(value: [string, bool, string]): bool {
                let taken = value[0];
                let selected = &value[1];
                return *selected;
            }
            function moved(value: [string, bool]): void {
                let taken = value[0];
                let view = &value[0];
                let _ = view;
            }
            function exclusive(seed: [bool, bool]): bool {
                let mut value = seed;
                let first = &mut value[0];
                let second = &value[1];
                let _ = *first;
                return *second;
            }
            function distinct(left: [bool, bool], right: [bool, bool]): bool {
                let mut a = left;
                let mut b = right;
                let first = &mut a[0];
                let second = &mut b[0];
                let _ = *first;
                return *second;
            }
            function writeWhileShared(seed: [bool, bool]): bool {
                let mut value = seed;
                let first = &value[0];
                value[1] = true;
                return *first;
            }
            function readWhileExclusive(seed: [bool, bool]): bool {
                let mut value = seed;
                let first = &mut value[0];
                let second = value[1];
                let _ = *first;
                return second;
            }
            function moveWhileShared(value: [bool, string]): string {
                let first = &value[0];
                let carried = first;
                let second = value[1];
                let _ = *carried;
                return second;
            }
            function lastUse(seed: [bool, bool]): bool {
                let mut value = seed;
                let first = &mut value[0];
                let _ = *first;
                value[1] = true;
                return value[1];
            }
            function expires(flag: bool, seed: &bool): bool {
                let mut held = seed;
                if (flag) { let pair: [bool, bool] = [false, true]; held = &pair[0]; }
                return *held;
            }
            function ends(flag: bool, seed: &bool): void {
                let mut held = seed;
                if (flag) {
                    let pair: [bool, bool] = [false, true];
                    held = &pair[0];
                    let _ = *held;
                }
            }
            function escape(value: [bool, bool]): &bool { return &value[0]; }
            function descriptorSlot(value: [&bool, bool]): &&bool { return &value[0]; }
            function borrowedTuple(value: &[bool, bool]): &bool { return &value[0]; }
            function nested(value: [[bool, bool], bool]): &bool { return &value[0][1]; }
            function groupedCycle(seed: &bool): void {
                let mut a: [bool, &bool] = [false, seed];
                let mut b: [bool, &bool] = [true, seed];
                a[1] = &b[0];
                b[1] = &a[0];
            }
            function oneWay(seed: &bool): void {
                let mut a: [bool, &bool] = [false, seed];
                let b: [bool, &bool] = [true, seed];
                a[1] = &b[0];
            }
            function liveCycle(seed: &bool, flag: bool): bool {
                let mut held = seed;
                if (flag) {
                    let mut a: [bool, &bool] = [false, seed];
                    let mut b: [bool, &bool] = [true, seed];
                    a[1] = &b[0];
                    b[1] = &a[0];
                    held = &a[0x0];
                }
                return *held;
            }
            function incompatibleOrder(seed: &bool): void {
                let mut a: [&bool, bool] = [seed, false];
                let mut b: [&bool, bool] = [seed, true];
                a[0] = &b[1];
                b[0] = &a[1];
            }
            function observableCycle(seed: &bool, first: string, second: string): void {
                let mut a: [bool, &bool, string] = [false, seed, first];
                let mut b: [bool, &bool, string] = [true, seed, second];
                a[1] = &b[0];
                b[1] = &a[0];
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
            let check = |name| lower(name, AnalysisLimits::default());
            let shared = check("shared");
            let mir = shared
                .mir()
                .unwrap_or_else(|| panic!("shared: {:?}", shared.completion()));
            let borrows: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Borrow { .. }))
                .collect();
            assert!(matches!(borrows.as_slice(), [
                Operation::Borrow { source: first, holder: a, exclusive: false },
                Operation::Borrow { source: second, holder: b, exclusive: false },
            ] if first.root.node == second.root.node && first.root.span.bytes() == b"value: [bool, bool, string]"
                && first.tuple_element() == Some(0) && second.tuple_element() == Some(1)
                && a.root.span.bytes() == b"first" && b.root.span.bytes() == b"second"));
            assert_eq!(dropped_elements(mir, mir.entry), [2]);
            let after_move = check("afterMove");
            let mir = after_move
                .mir()
                .expect("an initialized member remains borrowable after a sibling move");
            assert!(matches!(path_drops(mir, mir.entry).as_slice(),
                [Obligation::Drop(taken), Obligation::Drop(last)] if taken.root.span.bytes() == b"taken" && last.tuple_element() == Some(2)));
            let last_use = check("lastUse");
            let mir = last_use
                .mir()
                .expect("the exclusive restriction ends after the last use");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. }
                            | Operation::Dereference(_)
                            | Operation::Install { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source, holder, exclusive: true },
                Operation::Dereference(used),
                Operation::Install { target, .. },
                Operation::Export(_),
            ] if holder.root.node == used.root.node && source.root.node == target.root.node
                && source.tuple_element() == Some(0) && target.tuple_element() == Some(1)));

            for (name, code, site, related) in [
                ("moved", "R0200", "&value[0]", "value[0]"),
                ("exclusive", "R0201", "&value[1]", "&mut value[0]"),
                ("writeWhileShared", "R0203", "value[1] = true;", "&value[0]"),
                ("readWhileExclusive", "R0201", "value[1]", "&mut value[0]"),
                ("moveWhileShared", "R0203", "value[1]", "&value[0]"),
                (
                    "expires",
                    "R0203",
                    "{ let pair: [bool, bool] = [false, true]; held = &pair[0]; }",
                    "&pair[0]",
                ),
                ("escape", "R0202", "return &value[0];", "&value[0]"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one diagnostic")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            let distinct = check("distinct");
            let mir = distinct
                .mir()
                .expect("exclusive loans of different owners coexist");
            let borrows: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Borrow { .. }))
                .collect();
            assert!(matches!(borrows.as_slice(), [
                Operation::Borrow { source: first, exclusive: true, .. },
                Operation::Borrow { source: second, exclusive: true, .. },
            ] if first.root.node != second.root.node && first.tuple_element() == Some(0) && second.tuple_element() == Some(0)));
            assert!(check("ends").mir().is_some());
            let live = check("liveCycle");
            assert_eq!(live.completion(), Completion::SourceRejected);
            assert!(live.mir().is_none());
            let [finding] = live.findings() else {
                panic!("expected the external holder to block scope exit")
            };
            assert_eq!(finding.code(), "R0203");
            assert!(
                finding
                    .site
                    .span
                    .bytes()
                    .starts_with(b"{\n                    let mut a:")
            );
            assert_eq!(finding.related.span.bytes(), b"&a[0x0]");
            for name in [
                "descriptorSlot",
                "borrowedTuple",
                "nested",
                "incompatibleOrder",
                "observableCycle",
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                if matches!(name, "incompatibleOrder" | "observableCycle") {
                    let site = report.blocked_at().unwrap();
                    assert!(
                        site.span
                            .bytes()
                            .starts_with(b"{\n                let mut a:"),
                        "{name}"
                    );
                }
            }
            assert_eq!(
                check("descriptorSlot").blocked_at().unwrap().span.bytes(),
                b"&value[0]"
            );
            let order = |name| {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let scopes: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter_map(|op| match op {
                        Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action,
                            Obligation::EndStorage(place) if matches!(place.root.span.bytes(), b"a" | b"b"))) => Some(actions),
                        _ => None,
                    })
                     .collect();
                let [scope] = scopes.as_slice() else {
                    panic!("both tuples must end on the same boundary")
                };
                scope
                    .iter()
                    .map(|action| match action {
                        Obligation::EndStorage(place) => {
                            (place.root.span.bytes(), place.tuple_element())
                        }
                        other => panic!("unexpected cleanup: {other:?}"),
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                order("groupedCycle"),
                [
                    (b"b".as_slice(), Some(1)),
                    (b"a".as_slice(), Some(1)),
                    (b"b".as_slice(), Some(0)),
                    (b"b".as_slice(), None),
                    (b"a".as_slice(), Some(0)),
                    (b"a".as_slice(), None),
                ]
            );
            assert_eq!(
                order("oneWay"),
                [
                    (b"a".as_slice(), Some(1)),
                    (b"a".as_slice(), Some(0)),
                    (b"a".as_slice(), None),
                    (b"b".as_slice(), Some(1)),
                    (b"b".as_slice(), Some(0)),
                    (b"b".as_slice(), None),
                ]
            );

            let cycle = check("groupedCycle");
            let limits = AnalysisLimits {
                max_work: cycle.statistics().work,
                max_entries: cycle.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("groupedCycle", limits).mir().is_some());
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
                let limited = lower("groupedCycle", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn direct_calls_keep_exclusive_tuple_backing_only_in_borrowing_results() {
    probe(
        "tuples::direct_calls_keep_exclusive_tuple_backing_only_in_borrowing_results",
        r#"
            function project(parent: &mut [bool, string]): &mut string {
                let [_, view] = &mut *parent;
                return view;
            }
            function snapshot(parent: &mut [bool, string]): bool {
                let [bit, _] = &mut *parent;
                return bit;
            }
            function projectShared(parent: &mut [bool, string]): &string {
                let [_, view] = &mut *parent;
                return &*view;
            }
            function touch(view: &mut string): void {}
            function resumes(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let bit = snapshot(parent);
                let view = project(parent);
                touch(view);
                let _ = parent;
                owner[1] = "new";
                return bit;
            }
            function freshResumes(seed: [bool, string]): bool {
                let mut owner = seed;
                let view = project(&mut owner);
                touch(view);
                owner[1] = "new";
                return owner[0];
            }
            function inspect(parent: &mut [bool, string]): void {}
            function literalOwner(): bool {
                let mut owner: [bool, string] = [false, "initial"];
                let bit = snapshot(&mut owner);
                inspect(&mut owner);
                owner[1] = "new";
                return bit;
            }
            function literalNamed(): bool {
                let mut owner: [bool, string] = [false, "initial"];
                let parent = &mut owner;
                let bit = snapshot(parent);
                let _ = parent;
                owner[1] = "new";
                return bit;
            }
            function sharedStillExclusive(seed: [bool, string]): bool {
                let mut owner = seed;
                let view = projectShared(&mut owner);
                let bit = owner[0];
                let _ = view;
                return bit;
            }
            function liveFresh(seed: [bool, string]): bool {
                let mut owner = seed;
                let view = project(&mut owner);
                let bit = owner[0];
                touch(view);
                return bit;
            }
            function suspended(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let view = project(parent);
                let bit = snapshot((parent));
                touch(view);
                return bit;
            }
            function escapes(seed: [bool, string]): &mut string {
                let mut owner = seed;
                return project(&mut owner);
            }
            function expires(seed: [bool, string], fallback: &mut string, branch: bool): void {
                let mut out = fallback;
                if (branch) { let mut owner = seed; out = project(&mut owner); }
                touch(out);
            }
            function bad(parent: &mut [bool, string]): &mut string {
                let mut local: [bool, string] = [false, "local"];
                let [_, escaped] = &mut local;
                return escaped;
            }
            function reachedBad(seed: [bool, string]): void {
                let mut owner = seed;
                let _ = bad(&mut owner);
            }
            function containsReference(parent: &mut [&bool, string]): bool { return true; }
            function unsupportedContents(seed: [&bool, string]): bool {
                let mut owner = seed;
                return containsReference(&mut owner);
            }
            function compoundResult(parent: &mut [bool, string]): [bool, bool] { return [true, false]; }
            function unsupportedResult(seed: [bool, string]): [bool, bool] {
                let mut owner = seed;
                return compoundResult(&mut owner);
            }
            function explicitSnapshot(): bool {
                let mut owner: [bool, string] = [true, "initial"];
                let parent = &mut owner;
                let bit = snapshot(&mut *(parent));
                let _ = parent;
                owner[1] = "new";
                return bit;
            }
            function explicitChain(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let child = (&mut *parent);
                let view = project(&mut *child);
                touch(view);
                let _ = child;
                let _ = parent;
                owner[1] = "new";
                return owner[0];
            }
            function explicitSuspended(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let view = project(&mut *parent);
                let bit = snapshot(&mut *(parent));
                touch(view);
                return bit;
            }
            function explicitOwner(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let child = &mut *parent;
                let view = project(&mut *child);
                let bit = owner[0];
                touch(view);
                return bit;
            }
            function parentStillSuspended(seed: [bool, string], fallback: &mut string): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let mut view = fallback;
                { let child = &mut *parent; view = project(&mut *child); }
                let bit = snapshot(parent);
                touch(view);
                return bit;
            }
            function parentSlotEnds(seed: [bool, string], fallback: &mut string): bool {
                let mut owner = seed;
                let mut view = fallback;
                { let parent = &mut owner; view = project(&mut *parent); }
                let bit = owner[0];
                touch(view);
                return bit;
            }
            function explicitSharedResult(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let view = projectShared(&mut *parent);
                let bit = snapshot(parent);
                let _ = view;
                return bit;
            }
            function movedDescriptor(seed: [bool, string]): bool {
                let mut owner = seed;
                let parent = &mut owner;
                let moved = parent;
                return snapshot(&mut *parent);
            }
            function escapedDescriptor(seed: [bool, string]): &mut [bool, string] {
                let mut owner = seed;
                let parent = &mut owner;
                return &mut *parent;
            }
            function unsupportedIndirect(parent: &mut &mut [bool, string]): bool {
                return snapshot(&mut **parent);
            }
            function unsupportedReborrowContents(parent: &mut [&bool, string]): void {
                let child = &mut *parent;
                let _ = child;
            }
            function unsupportedSharedReborrow(parent: &mut [bool, string]): void {
                let child = &*parent;
                let _ = child;
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
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let check = |name| lower(name, AnalysisLimits::default());
            let resumes = check("resumes");
            let mir = resumes
                .mir()
                .unwrap_or_else(|| panic!("resumes: {:?}", resumes.completion()));
            fn accesses<'a, 'g>(mir: &'a Mir<'g>) -> Vec<&'a Operation<'g>> {
                linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::Borrow { .. }
                                | Operation::Reborrow { .. }
                                | Operation::Call(_)
                                | Operation::Install { .. }
                        )
                    })
                    .collect()
            }
            let route = accesses(mir);
            assert!(
                matches!(route.as_slice(), [
                Operation::Borrow { source, holder: parent, exclusive: true },
                Operation::Reborrow { parent: scalar, exclusive: true, .. },
                Operation::Call(snapshot),
                Operation::Reborrow { parent: reference, exclusive: true, .. },
                Operation::Call(project),
                Operation::Reborrow { parent: view, exclusive: true, .. },
                Operation::Call(touch),
                Operation::Install { target, .. },
            ] if source.projection.is_none() && target.tuple_element() == Some(1)
                && source.root.node == target.root.node && parent.root.node == scalar.root.node && parent.root.node == reference.root.node
                && parent.root.span.bytes() == b"parent" && view.root.span.bytes() == b"view"
                && **snapshot == declaration("snapshot").id && **project == declaration("project").id && **touch == declaration("touch").id),
                "named tuple call path: {route:?}"
            );
            let explicit = check("explicitChain");
            let limits = AnalysisLimits {
                max_work: explicit.statistics().work,
                max_entries: explicit.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("explicitChain", limits).mir().is_some());
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
                let limited = lower("explicitChain", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(old), Obligation::Drop(replacement)]
                if old.root.node == replacement.root.node && old.root.span.bytes() == b"owner"
                    && old.tuple_element() == Some(1) && replacement.tuple_element() == Some(1))
            );
            for name in ["literalOwner", "literalNamed"] {
                let literal = check(name);
                let mir = literal
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", literal.completion()));
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(old), Obligation::Drop(new)]
                    if old.root.node == new.root.node && old.root.span.bytes() == b"owner"
                        && old.tuple_element() == Some(1) && new.tuple_element() == Some(1))
                );
            }
            for name in ["explicitSnapshot", "explicitChain"] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let route = accesses(mir);
                if name == "explicitSnapshot" {
                    assert!(
                        matches!(route.as_slice(), [
                        Operation::Borrow { source, holder: original, exclusive: true },
                        Operation::Reborrow { parent, holder: staged, exclusive: true },
                        Operation::Call(callee),
                        Operation::Install { target, .. },
                    ] if source.projection.is_none() && source.root.node == target.root.node && target.tuple_element() == Some(1)
                        && original.root.node == parent.root.node && staged.root.span.bytes() == b"&mut *(parent)"
                        && **callee == declaration("snapshot").id),
                        "{name}: {route:?}"
                    );
                } else {
                    assert!(
                        matches!(route.as_slice(), [
                        Operation::Borrow { source, holder: original, exclusive: true },
                        Operation::Reborrow { parent, holder: local, exclusive: true },
                        Operation::Reborrow { parent: child, holder: staged, exclusive: true },
                        Operation::Call(project),
                        Operation::Reborrow { parent: view, exclusive: true, .. },
                        Operation::Call(touch),
                        Operation::Install { target, .. },
                    ] if source.projection.is_none() && source.root.node == target.root.node && target.tuple_element() == Some(1)
                        && original.root.node == parent.root.node && local.root.node == child.root.node
                        && staged.root.span.bytes() == b"&mut *child" && view.root.span.bytes() == b"view"
                        && **project == declaration("project").id && **touch == declaration("touch").id),
                        "{name}: {route:?}"
                    );
                }
                assert!(linear(mir, mir.entry).windows(2).any(|step| matches!(step,
                    [Operation::Drop(old), Operation::Install { target, .. }]
                        if old.root.node == target.root.node && old.root.span.bytes() == b"owner"
                            && old.tuple_element() == Some(1) && target.tuple_element() == Some(1))),
                    "{name}: the displaced member must drop before installation");
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(old), Obligation::Drop(new)]
                    if old.root.node == new.root.node && old.root.span.bytes() == b"owner"
                        && old.tuple_element() == Some(1) && new.tuple_element() == Some(1))
                );
            }
            let fresh = check("freshResumes");
            let mir = fresh
                .mir()
                .expect("the fresh call's loan ends after the returned view is dead");
            let route = accesses(mir);
            assert!(
                matches!(route.as_slice(), [
                Operation::Borrow { source, exclusive: true, .. },
                Operation::Call(project),
                Operation::Reborrow { parent: view, exclusive: true, .. },
                Operation::Call(touch),
                Operation::Install { target, .. },
            ] if source.projection.is_none() && source.root.node == target.root.node && target.tuple_element() == Some(1)
                && view.root.span.bytes() == b"view" && **project == declaration("project").id && **touch == declaration("touch").id),
                "fresh tuple call path: {route:?}"
            );
            for (name, code, at, cause) in [
                ("liveFresh", "R0201", "owner[0]", "&mut owner"),
                ("sharedStillExclusive", "R0201", "owner[0]", "&mut owner"),
                ("suspended", "R0201", "(parent)", "parent"),
                (
                    "explicitSuspended",
                    "R0201",
                    "&mut *(parent)",
                    "&mut *parent",
                ),
                ("explicitOwner", "R0201", "owner[0]", "&mut owner"),
                ("parentStillSuspended", "R0201", "parent", "&mut *parent"),
                ("parentSlotEnds", "R0201", "owner[0]", "&mut owner"),
                ("explicitSharedResult", "R0201", "parent", "&mut *parent"),
                ("movedDescriptor", "R0200", "&mut *parent", "parent"),
                (
                    "escapedDescriptor",
                    "R0202",
                    "return &mut *parent;",
                    "owner",
                ),
                ("escapes", "R0202", "return project(&mut owner);", "owner"),
                (
                    "expires",
                    "R0203",
                    "{ let mut owner = seed; out = project(&mut owner); }",
                    "&mut owner",
                ),
                ("reachedBad", "R0202", "return escaped;", "local"),
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
            for (name, site) in [
                ("unsupportedContents", "&mut owner"),
                ("unsupportedResult", "&mut owner"),
                ("unsupportedIndirect", "**parent"),
                ("unsupportedReborrowContents", "*parent"),
                ("unsupportedSharedReborrow", "*parent"),
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
fn direct_nested_exclusive_tuple_calls_keep_the_incoming_restriction() {
    probe(
        "tuples::direct_nested_exclusive_tuple_calls_keep_the_incoming_restriction",
        r#"
            function project(parent: &mut [[bool, string], [bool, i32]]): &mut string {
                let [[_, view], _] = &mut *parent;
                return view;
            }
            function readOnly(parent: &mut [[bool, string], [bool, i32]]): &string {
                let [[_, view], _] = &mut *parent;
                return &*view;
            }
            function snapshot(parent: &mut [[bool, string], [bool, i32]]): bool {
                let [_, [bit, _]] = &mut *parent;
                return bit;
            }
            function useShared(view: &string): void {}
            function useExclusive(view: &mut string): void {}
            function returns(parent: &mut [[bool, string], [bool, i32]]): &mut string {
                return project((parent));
            }
            function resumes(parent: &mut [[bool, string], [bool, i32]]): bool {
                let first = snapshot(parent);
                let view = readOnly((parent));
                useShared(view);
                let bit = snapshot(parent);
                let moved = parent;
                return bit;
            }
            function sharedBlocked(parent: &mut [[bool, string], [bool, i32]]): bool {
                let view = readOnly(parent);
                let copied = view;
                let bit = snapshot((parent));
                useShared(copied);
                return bit;
            }
            function observeBlocked(parent: &mut [[bool, string], [bool, i32]]): void {
                let view = readOnly(parent);
                let copied = view;
                let observer = &(parent);
                useShared(copied);
            }
            function observeAfter(parent: &mut [[bool, string], [bool, i32]]): void {
                let view = readOnly(parent);
                let copied = view;
                useShared(copied);
                let observer = &(parent);
            }
            function exclusiveBlocked(parent: &mut [[bool, string], [bool, i32]]): void {
                let view = project(parent);
                let movedView = view;
                let moved = parent;
                useExclusive(movedView);
            }
            function movedParent(parent: &mut [[bool, string], [bool, i32]]): bool {
                let moved = parent;
                return snapshot((parent));
            }
            function bad(parent: &mut [[bool, string], [bool, i32]]): &mut string {
                let mut owner = "local";
                return &mut owner;
            }
            function reachedBad(parent: &mut [[bool, string], [bool, i32]]): &mut string {
                return bad(parent);
            }
            function explicit(parent: &mut [[bool, string], [bool, i32]]): bool {
                return snapshot(&mut *parent);
            }
            function alias(parent: &mut [[bool, string], [bool, i32]]): bool {
                let named = parent;
                return snapshot(named);
            }
            function tupleResult(parent: &mut [[bool, string], [bool, i32]]): [bool, bool] { return [true, false]; }
            function compound(parent: &mut [[bool, string], [bool, i32]]): [bool, bool] {
                return tupleResult(parent);
            }
            function inputWithReference(parent: &mut [[bool, &string], i32]): bool { return true; }
            function referenceLeaf(parent: &mut [[bool, &string], i32]): bool {
                return inputWithReference(parent);
            }
            function thirdLevel(parent: &mut [[[bool, string]], i32]): bool { return true; }
            function deeper(parent: &mut [[[bool, string]], i32]): bool {
                return thirdLevel(parent);
            }
            function narrowStep(parent: &mut [[bool], i32]): bool { return true; }
            function wideStep(parent: &mut [[bool, string, i32], i32]): bool { return true; }
            function narrow(parent: &mut [[bool], i32]): bool { return narrowStep(parent); }
            function wide(parent: &mut [[bool, string, i32], i32]): bool { return wideStep(parent); }
            function narrowUnary(parent: &mut [[bool], i32]): bool { return parent |> narrowStep; }
            function wideUnary(parent: &mut [[bool, string, i32], i32]): bool { return parent |> wideStep; }
            function narrowPair(other: &[bool], parent: &mut [[bool], i32]): bool { return true; }
            function widePair(other: &[bool], parent: &mut [[bool, string, i32], i32]): bool { return true; }
            function narrowPipe(parent: &mut [[bool], i32], other: &[bool]): bool { return parent |> narrowPair(other, _); }
            function widePipe(parent: &mut [[bool, string, i32], i32], other: &[bool]): bool { return parent |> widePair(other, _); }
        "#,
        |_, module, receipt| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let returns = lower("returns", default);
            let mir = returns
                .mir()
                .unwrap_or_else(|| panic!("nested exclusive call: {:?}", returns.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. } | Operation::Call(_) | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Reborrow { parent, holder, exclusive: true }, Operation::Call(callee), Operation::Export(result)]
                if parent.root.span.bytes() == b"parent: &mut [[bool, string], [bool, i32]]" && holder.root.span.bytes() == b"(parent)" && *callee == id("project")
                    && parent.projection.is_none() && holder.projection.is_none() && result.projection.is_none()
                    && result.root.span.bytes() == b"project((parent))")
            );
            let resumes = lower("resumes", default);
            let mir = resumes
                .mir()
                .expect("scalar result and last use release the staged parent");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::Call(_)
                            | Operation::Move { .. }
                            | Operation::Copy { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Reborrow { parent, holder: first, exclusive: true }, Operation::Call(first_call),
                Operation::Reborrow { parent: second, holder: shared, exclusive: true }, Operation::Call(second_call),
                Operation::Copy { from: view, .. }, Operation::Call(use_call),
                Operation::Reborrow { parent: third, holder: last, exclusive: true }, Operation::Call(last_call),
                Operation::Move { from: original, into: Some(moved) },
                Operation::Copy { from: bit, into: Some(staged) }, Operation::Export(output),
            ] if parent.root.node == second.root.node && parent.root.node == third.root.node && parent.root.node == original.root.node
                && first.root.span.bytes() == b"parent" && shared.root.span.bytes() == b"(parent)" && last.root.span.bytes() == b"parent"
                && [parent, second, third, first, shared, last].iter().all(|place| place.projection.is_none())
                && *first_call == id("snapshot") && *second_call == id("readOnly") && *use_call == id("useShared") && *last_call == id("snapshot")
                && view.root.span.bytes() == b"view" && bit.root.span.bytes() == b"bit" && staged.root.node == output.root.node && moved.root.span.bytes() == b"moved"));
            let observe = lower("observeAfter", default);
            let mir = observe
                .mir()
                .expect("read-only access resumes after the copied view's last use");
            let ops: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. } | Operation::Borrow { .. } | Operation::Call(_)
                    )
                })
                .collect();
            assert!(matches!(ops.as_slice(), [
                Operation::Reborrow { parent, exclusive: true, .. }, Operation::Call(project), Operation::Call(read),
                Operation::Borrow { source, exclusive: false, .. },
            ] if parent.root.node == source.root.node && source.projection.is_none() && *project == id("readOnly") && *read == id("useShared")));
            for (name, code, at, cause) in [
                (
                    "sharedBlocked",
                    "R0201",
                    b"(parent)".as_slice(),
                    b"parent".as_slice(),
                ),
                ("observeBlocked", "R0201", b"&(parent)", b"parent"),
                ("exclusiveBlocked", "R0203", b"parent", b"parent"),
                ("movedParent", "R0200", b"(parent)", b"parent"),
                ("reachedBad", "R0202", b"return &mut owner;", b"owner"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), at, "{name}");
                assert_eq!(finding.related.span.bytes(), cause, "{name}");
                assert!(
                    finding.related.span.start() < finding.site.span.start(),
                    "{name}: cause precedes the conflicting use"
                );
            }
            for (name, at) in [
                ("explicit", b"&mut *parent".as_slice()),
                ("alias", b"named"),
                ("compound", b"parent"),
                ("referenceLeaf", b"parent"),
                ("deeper", b"parent"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), at, "{name}");
            }
            let narrow = lower("narrow", default);
            let wide = lower("wide", default);
            assert!(wide.mir().is_some() && narrow.mir().is_some());
            assert!(wide.statistics().work >= narrow.statistics().work + 4);
            assert_eq!(wide.statistics().entries, narrow.statistics().entries);
            assert_eq!(wide.statistics().max_depth, narrow.statistics().max_depth);
            let narrow_unary = lower("narrowUnary", default);
            let wide_unary = lower("wideUnary", default);
            assert!(narrow_unary.mir().is_some() && wide_unary.mir().is_some());
            assert_eq!(
                wide_unary.statistics().work - narrow_unary.statistics().work,
                wide.statistics().work - narrow.statistics().work
            );
            assert_eq!(
                wide_unary.statistics().entries,
                narrow_unary.statistics().entries
            );
            assert_eq!(
                wide_unary.statistics().max_depth,
                narrow_unary.statistics().max_depth
            );
            let narrow_pipe = lower("narrowPipe", default);
            let wide_pipe = lower("widePipe", default);
            for pipe in [&narrow_pipe, &wide_pipe] {
                assert_eq!(pipe.completion(), Completion::Incomplete);
                assert!(pipe.mir().is_none());
            }
            assert_eq!(wide_pipe.statistics().work, narrow_pipe.statistics().work);
            assert_exact_tuple_call_limits(&resumes, |limits| lower("resumes", limits));
        },
    );
}

#[test]
fn unary_pipes_reborrow_one_incoming_nested_exclusive_descriptor() {
    probe(
        "tuples::unary_pipes_reborrow_one_incoming_nested_exclusive_descriptor",
        r#"
            function project(parent: &mut [[bool, string], i32]): &mut string {
                let [[_, view], _] = &mut *parent;
                return view;
            }
            function readOnly(parent: &mut [[bool, string], i32]): &string {
                let [[_, view], _] = &mut *parent;
                return &*view;
            }
            function snapshot(parent: &mut [[bool, string], i32]): bool {
                let [[bit, _], _] = &mut *parent;
                return bit;
            }
            function touch(view: &string): void {}
            function returns(parent: &mut [[bool, string], i32]): &mut string {
                return (parent) |> project(_);
            }
            function resumes(parent: &mut [[bool, string], i32]): bool {
                let first = parent |> snapshot;
                let view = (parent) |> readOnly(_);
                let copied = view;
                touch(copied);
                let observer = &(parent);
                let bit = parent |> snapshot(_);
                let moved = parent;
                return bit;
            }
            function blocked(parent: &mut [[bool, string], i32]): void {
                let view = parent |> readOnly;
                let copied = view;
                let observer = &(parent);
                touch(copied);
            }
            function consumed(parent: &mut [[bool, string], i32]): bool {
                let moved = parent;
                return (parent) |> snapshot;
            }
            function bad(parent: &mut [[bool, string], i32]): &mut string {
                let mut owner = "local";
                return &mut owner;
            }
            function reachedBad(parent: &mut [[bool, string], i32]): &mut string {
                return parent |> bad(_);
            }
            function alias(parent: &mut [[bool, string], i32]): bool {
                let named = parent;
                return named |> snapshot;
            }
            function explicit(parent: &mut [[bool, string], i32]): bool {
                return (&mut *parent) |> snapshot;
            }
            function maybe(parent: &mut [[bool, string], i32]): Option<bool> { return Option.Some(true); }
            function compound(parent: &mut [[bool, string], i32]): Option<bool> {
                return parent |> maybe;
            }
            function propagating(parent: &mut [[bool, string], i32]): Option<bool> {
                let bit = parent |> maybe?;
                return Option.Some(bit);
            }
        "#,
        |_, module, receipt| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let returns = lower("returns", default);
            let mir = returns
                .mir()
                .unwrap_or_else(|| panic!("nested exclusive pipe: {:?}", returns.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. } | Operation::Call(_) | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Reborrow { parent, holder, exclusive: true }, Operation::Call(callee), Operation::Export(output)]
                if parent.root.span.bytes() == b"parent: &mut [[bool, string], i32]" && holder.root.span.bytes() == b"(parent)"
                    && parent.projection.is_none() && holder.projection.is_none() && *callee == id("project")
                    && output.root.span.bytes() == b"(parent) |> project(_)")
            );
            let resumes = lower("resumes", default);
            let mir = resumes
                .mir()
                .expect("the scalar return and last use both release the staged parent");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::Borrow { .. }
                            | Operation::Call(_)
                            | Operation::Move { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Reborrow { parent, holder: first, exclusive: true }, Operation::Call(before),
                Operation::Reborrow { parent: second, holder: shared, exclusive: true }, Operation::Call(read_only), Operation::Call(touch),
                Operation::Borrow { source, exclusive: false, .. },
                Operation::Reborrow { parent: third, holder: last, exclusive: true }, Operation::Call(after),
                Operation::Move { from: original, into: Some(moved) }, Operation::Export(result),
            ] if [second, source, third, original].iter().all(|place| place.root.node == parent.root.node && place.projection.is_none())
                && [parent, first, shared, last].iter().all(|place| place.projection.is_none()) && first.root.span.bytes() == b"parent"
                && shared.root.span.bytes() == b"(parent)" && last.root.span.bytes() == b"parent"
                && *before == id("snapshot") && *read_only == id("readOnly") && *touch == id("touch") && *after == id("snapshot")
                && moved.root.span.bytes() == b"moved" && result.root.span.bytes() == b"bit"));
            for (name, code, site, cause) in [
                (
                    "blocked",
                    "R0201",
                    b"&(parent)".as_slice(),
                    b"parent".as_slice(),
                ),
                ("consumed", "R0200", b"(parent)", b"parent"),
                ("reachedBad", "R0202", b"return &mut owner;", b"owner"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), cause);
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            for (name, at) in [
                ("alias", b"named |> snapshot".as_slice()),
                ("explicit", b"(&mut *parent) |> snapshot"),
                ("compound", b"parent |> maybe"),
                ("propagating", b"parent |> maybe?"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), at, "{name}");
            }
            assert_exact_tuple_call_limits(&resumes, |limits| lower("resumes", limits));
        },
    );
}

fn assert_exact_tuple_call_limits<'g>(
    report: &compiler::analysis::ownership::Report<'g>,
    lower: impl Fn(AnalysisLimits) -> compiler::analysis::ownership::Report<'g>,
) {
    let statistics = report.statistics();
    let exact = AnalysisLimits {
        max_work: statistics.work,
        max_entries: statistics.entries,
        max_depth: statistics.max_depth,
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
}

#[test]
fn nested_exclusive_calls_with_leaf_arguments_keep_both_backings() {
    probe(
        "tuples::nested_exclusive_calls_with_leaf_arguments_keep_both_backings",
        r#"
            function mark(): bool { return true; }
            function select(flag: bool, fallback: &string, parent: &mut [[bool, string], i32]): &string {
                let [[_, view], _] = &mut *parent;
                return &*view;
            }
            function scalar(flag: bool, fallback: &string, parent: &mut [[bool, string], i32]): bool {
                let [[bit, _], _] = &mut *parent;
                return bit;
            }
            function touch(view: &string): void {}
            function forward(parent: &mut [[bool, string], i32], fallback: &string): &string {
                return select(mark(), fallback, (parent));
            }
            function pipeForward(parent: &mut [[bool, string], i32], fallback: &string): &string {
                return (parent) |> select(mark(), fallback, _);
            }
            function leading(parent: &mut [[bool, string], i32], flag: bool, fallback: &string): &string {
                return select(flag, fallback, parent);
            }
            function implicit(parent: &mut [[bool, string], i32], fallback: &string): &string {
                return (parent) |> leading(mark(), fallback);
            }
            function releases(parent: &mut [[bool, string], i32]): bool {
                let first = "first";
                let bit = parent |> scalar(mark(), &first, _);
                dispose(first);
                let fallback = "fallback";
                let view = parent |> select(mark(), &fallback, _);
                touch(view);
                dispose(fallback);
                let moved = parent;
                return bit;
            }
            function keepsParent(parent: &mut [[bool, string], i32], fallback: &string): void {
                let view = parent |> select(mark(), fallback, _);
                let copied = view;
                let observer = &parent;
                touch(copied);
            }
            function keepsFallback(parent: &mut [[bool, string], i32]): void {
                let fallback = "local";
                let view = parent |> select(mark(), &fallback, _);
                dispose(fallback);
                touch(view);
            }
            function escapeFallback(parent: &mut [[bool, string], i32]): &string {
                let fallback = "local";
                return select(mark(), &fallback, parent);
            }
            function bad(flag: bool, fallback: &string, parent: &mut [[bool, string], i32]): &string {
                let owner = "local";
                return &owner;
            }
            function reachedBad(parent: &mut [[bool, string], i32], fallback: &string): &string {
                return parent |> bad(mark(), fallback, _);
            }
            function nestedFirst(parent: &mut [[bool, string], i32], other: &[bool, string]): bool { return true; }
            function nestedLast(other: &[bool, string], parent: &mut [[bool, string], i32]): bool { return true; }
            function twoFirst(parent: &mut [[bool, string], i32], other: &[bool, string]): bool {
                return nestedFirst(parent, other);
            }
            function twoLast(parent: &mut [[bool, string], i32], other: &[bool, string]): bool {
                return parent |> nestedLast(other, _);
            }
            function twoImplicit(parent: &mut [[bool, string], i32], other: &[bool, string]): bool {
                return parent |> nestedFirst(other);
            }
            function mutableExtra(parent: &mut [[bool, string], i32], extra: &mut bool): bool { return true; }
            function unsupportedMutable(parent: &mut [[bool, string], i32], extra: &mut bool): bool {
                return mutableExtra(parent, extra);
            }
        "#,
        |_, module, receipt| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            for (name, piped, expected_callee, source) in [
                (
                    "forward",
                    false,
                    "select",
                    b"select(mark(), fallback, (parent))".as_slice(),
                ),
                (
                    "pipeForward",
                    true,
                    "select",
                    b"(parent) |> select(mark(), fallback, _)",
                ),
                (
                    "implicit",
                    true,
                    "leading",
                    b"(parent) |> leading(mark(), fallback)",
                ),
            ] {
                let report = lower(name, default);
                let mir = report.mir().unwrap_or_else(|| {
                    panic!("nested leaf arguments {name}: {:?}", report.completion())
                });
                let ops: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::Call(_)
                                | Operation::Reborrow { .. }
                                | Operation::Copy { .. }
                                | Operation::Export(_)
                        )
                    })
                    .collect();
                let [
                    input,
                    mark,
                    fallback,
                    Operation::Call(callee),
                    Operation::Export(out),
                ] = ops.as_slice()
                else {
                    panic!("expected one operand of each kind and the final call/export")
                };
                let (input, mark, fallback) = if piped {
                    (input, mark, fallback)
                } else {
                    (fallback, input, mark)
                };
                assert!(
                    matches!(input, Operation::Reborrow { parent, holder, exclusive: true }
                    if parent.root.span.bytes() == b"parent: &mut [[bool, string], i32]" && holder.root.span.bytes() == b"(parent)"
                        && parent.projection.is_none() && holder.projection.is_none())
                );
                assert!(matches!(mark, Operation::Call(callee) if *callee == id("mark")));
                assert!(
                    matches!(fallback, Operation::Copy { from, into: Some(slot) }
                    if from.root.span.bytes() == b"fallback: &string" && slot.root.span.bytes() == b"fallback")
                );
                assert_eq!(*callee, id(expected_callee));
                assert_eq!(out.root.span.bytes(), source);
                assert!(path_drops(mir, mir.entry).is_empty());
            }
            let releases = lower("releases", default);
            let mir = releases
                .mir()
                .expect("scalar release and reference last use restore both sources");
            let ops: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::Borrow { .. }
                            | Operation::Call(_)
                            | Operation::Dispose { .. }
                            | Operation::Move { .. }
                    )
                })
                .collect();
            assert!(matches!(ops.as_slice(), [
                Operation::Reborrow { parent, exclusive: true, .. }, Operation::Call(mark_a), Operation::Borrow { source: first, exclusive: false, .. }, Operation::Call(scalar), Operation::Dispose { operand: disposed_first, .. },
                Operation::Reborrow { parent: again, exclusive: true, .. }, Operation::Call(mark_b), Operation::Borrow { source: fallback, exclusive: false, .. }, Operation::Call(select), Operation::Call(touch),
                Operation::Dispose { operand: disposed_fallback, .. }, Operation::Move { from, .. },
            ] if [again, from].iter().all(|place| place.root.node == parent.root.node) && first.root.node == disposed_first.root.node
                && fallback.root.node == disposed_fallback.root.node && *mark_a == id("mark") && *mark_b == id("mark")
                && *scalar == id("scalar") && *select == id("select") && *touch == id("touch")));
            for (name, code, site, cause) in [
                (
                    "keepsParent",
                    "R0201",
                    b"&parent".as_slice(),
                    b"parent".as_slice(),
                ),
                ("keepsFallback", "R0203", b"dispose(fallback)", b"&fallback"),
                (
                    "escapeFallback",
                    "R0202",
                    b"return select(mark(), &fallback, parent);",
                    b"fallback",
                ),
                ("reachedBad", "R0202", b"return &owner;", b"owner"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), cause);
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            for (name, site) in [
                ("twoFirst", b"parent".as_slice()),
                ("twoLast", b"parent |> nestedLast(other, _)"),
                ("twoImplicit", b"parent |> nestedFirst(other)"),
                ("unsupportedMutable", b"parent"),
            ] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site);
            }
            assert_exact_tuple_call_limits(&releases, |limits| lower("releases", limits));
        },
    );
}

#[test]
fn direct_shared_tuple_calls_keep_only_the_result_backing() {
    probe(
        "tuples::direct_shared_tuple_calls_keep_only_the_result_backing",
        r#"
            function nestedView(value: &[[bool, string], bool]): &string {
                let [[_, text], _] = value;
                return text;
            }
            function caller(value: &[[bool, string], bool]): &string { return nestedView(value); }
            function forwardsAlias(value: &[[bool, string], bool], fallback: &string): &string {
                let mut out = fallback;
                { let alias = value; out = nestedView(alias); }
                return out;
            }
            function snapshot(value: &[bool, string]): bool {
                let [bit, _] = value;
                return bit;
            }
            function view(value: &[bool, string]): &string {
                let [_, text] = value;
                return text;
            }
            function resumes(seed: [bool, string]): bool {
                let mut owner = seed;
                let bit = snapshot(&owner);
                owner[1] = "staged";
                let alias = &owner;
                let text = view(alias);
                let _ = text;
                owner[1] = "replaced";
                return bit;
            }
            function live(seed: [bool, string]): void {
                let mut owner = seed;
                let text = view(&owner);
                owner[1] = "blocked";
                let _ = text;
            }
            function escapes(seed: [bool, string]): &string { return view(&seed); }
            function expires(seed: [bool, string], fallback: &string, branch: bool): &string {
                let mut out = fallback;
                if (branch) { let local = seed; out = view(&local); }
                return out;
            }
            function partial(seed: [bool, string]): &string {
                let moved = seed[1];
                return view(&seed);
            }
            function bad(value: &[[bool, string], bool]): &string {
                let local = "local";
                return &local;
            }
            function reachedBad(value: &[[bool, string], bool]): &string { return bad(value); }
            function deeper(value: &[[[bool, string], bool], bool]): bool { return false; }
            function unsupportedDeep(value: &[[[bool, string], bool], bool]): bool { return deeper(value); }
            function carried(value: &[&bool, string]): bool { return false; }
            function unsupportedCarried(value: &[&bool, string]): bool { return carried(value); }
            function compoundResult(value: &[[bool, string], bool]): [bool, bool] { return [false, true]; }
            function unsupportedResult(value: &[[bool, string], bool]): [bool, bool] { return compoundResult(value); }
            function outerReference(value: &[&bool, [bool, bool]]): bool { return false; }
            function unsupportedOuterReference(value: &[&bool, [bool, bool]]): bool { return outerReference(value); }
            function innerReference(value: &[[&bool, string], bool]): bool { return false; }
            function unsupportedInnerReference(value: &[[&bool, string], bool]): bool { return innerReference(value); }
            function narrow(value: &[[bool, bool], bool]): bool { return true; }
            function wide(value: &[[bool, bool, bool, bool], bool]): bool { return true; }
            function callNarrow(value: &[[bool, bool], bool]): bool { return narrow(value); }
            function callWide(value: &[[bool, bool, bool, bool], bool]): bool { return wide(value); }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let check = |name| lower(name, AnalysisLimits::default());
            let caller = check("caller");
            let mir = caller
                .mir()
                .unwrap_or_else(|| panic!("caller: {:?}", caller.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::Call(_) | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Copy { from, into: Some(argument) }, Operation::Call(callee), Operation::Export(result),
            ] if from.root.span.bytes() == b"value: &[[bool, string], bool]" && argument.root.span.bytes() == b"value"
                && **callee == declaration("nestedView").id && result.root.span.bytes() == b"nestedView(value)"),
                "{route:?}"
            );
            let limits = AnalysisLimits {
                max_work: caller.statistics().work,
                max_entries: caller.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("caller", limits).mir().is_some());
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
                let limited = lower("caller", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            let narrow = check("callNarrow");
            let wide = check("callWide");
            assert!(narrow.mir().is_some() && wide.mir().is_some());
            // Two more leaves must contribute to both the shape scan and type equality.
            assert!(wide.statistics().work >= narrow.statistics().work + 4);

            let scoped = check("forwardsAlias");
            let mir = scoped
                .mir()
                .expect("the returned view does not borrow the local descriptor slot");
            let blocks = straight_blocks(mir, mir.entry);
            let ended = blocks.iter().position(|block| matches!(&block.operation, Operation::Cleanup(actions) if actions.iter().any(|action|
                matches!(action, Obligation::EndStorage(place) if place.root.span.bytes() == b"alias")))).unwrap();
            let transfer: Vec<_> = blocks[..ended]
                .iter()
                .copied()
                .filter(|block| {
                    matches!(
                        block.operation,
                        Operation::Call(_) | Operation::Install { .. }
                    )
                })
                .collect();
            let [called, stored] = transfer.as_slice() else {
                panic!("expected call and installation before alias cleanup")
            };
            assert!(
                matches!(called.operation, Operation::Call(callee) if *callee == declaration("nestedView").id)
            );
            let Operation::Install { target, incoming } = stored.operation else {
                panic!("expected installation of the view")
            };
            assert_eq!(incoming.root.node, called.site.node);
            assert_eq!(target.root.span.bytes(), b"out");
            assert!(
                matches!(blocks[ended + 1..].iter().map(|block| &block.operation)
                .filter(|op| matches!(op, Operation::Copy { .. } | Operation::Export(_))).collect::<Vec<_>>().as_slice(), [
                Operation::Copy { from, into: Some(result) }, Operation::Export(out)
            ] if from.root.node == target.root.node && result.root.node == out.root.node)
            );
            let resumes = check("resumes");
            let mir = resumes
                .mir()
                .expect("both call-scoped loans end after their last use");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Borrow { .. }
                    | Operation::Call(_)
                    | Operation::Drop(_)
                    | Operation::Install { .. } => true,
                    Operation::Copy { from, .. } => {
                        from.root.span.bytes() == b"alias" || from.root.span.bytes() == b"text"
                    }
                    _ => false,
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Borrow { source, exclusive: false, .. }, Operation::Call(snapshot),
                Operation::Drop(first), Operation::Install { target: staged, .. },
                Operation::Borrow { source: original, holder: descriptor, exclusive: false },
                Operation::Copy { from: alias, .. }, Operation::Call(view), Operation::Copy { from: text, .. },
                Operation::Drop(second), Operation::Install { target: replaced, .. },
            ] if source.projection.is_none() && original.projection.is_none() && source.root.node == original.root.node
                && [first, staged, second, replaced].iter().all(|place| place.root.node == source.root.node && place.tuple_element() == Some(1))
                && descriptor.root.node == alias.root.node && text.root.span.bytes() == b"text"
                && **snapshot == declaration("snapshot").id && **view == declaration("view").id),
                "{route:?}"
            );
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(first), Obligation::Drop(second), Obligation::Drop(last)]
                if first.root.node == second.root.node && second.root.node == last.root.node
                    && first.root.span.bytes() == b"owner" && [first, second, last].iter().all(|place| place.tuple_element() == Some(1)))
            );
            for (name, code, site, related) in [
                ("live", "R0203", "owner[1] = \"blocked\";", "&owner"),
                (
                    "escapes",
                    "R0202",
                    "return view(&seed);",
                    "seed: [bool, string]",
                ),
                ("partial", "R0200", "&seed", "seed[1]"),
                (
                    "expires",
                    "R0203",
                    "{ let local = seed; out = view(&local); }",
                    "&local",
                ),
                ("reachedBad", "R0202", "return &local;", "local"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for name in [
                "unsupportedDeep",
                "unsupportedCarried",
                "unsupportedResult",
                "unsupportedOuterReference",
                "unsupportedInnerReference",
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    b"value",
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn tuple_calls_with_leaf_arguments_keep_all_possible_backing() {
    probe(
        "tuples::tuple_calls_with_leaf_arguments_keep_all_possible_backing",
        r#"
            function choose(flag: bool, value: &[bool, string], fallback: &string): &string {
                let [_, text] = value;
                if (flag) { return text; }
                return fallback;
            }
            function nestedChoice(value: &[[bool, string], bool], flag: bool, fallback: &string): &string {
                let [[_, text], _] = value;
                if (flag) { return text; }
                return fallback;
            }
            function forward(value: &[[bool, string], bool], flag: bool, fallback: &string): &string {
                return nestedChoice(value, flag, fallback);
            }
            function view(value: &mut [bool, string], fallback: &string): &string {
                let [_, text] = &mut *value;
                return &*text;
            }
            function reversed(fallback: &string, value: &mut [bool, string]): &string {
                let [_, text] = &mut *value;
                return &*text;
            }
            function scalar(flag: bool, value: &mut [bool, string], fallback: &string): bool {
                let [bit, _] = &mut *value;
                return bit;
            }
            function resumes(seed: [bool, string], fallback: string, flag: bool): bool {
                let mut owner = seed;
                let bit = scalar(flag, &mut owner, &fallback);
                owner[1] = "staged";
                let text = choose(flag, &owner, &fallback);
                let _ = text;
                owner[1] = "replaced";
                dispose(fallback);
                return bit;
            }
            function keepsTuple(seed: [bool, string], fallback: &string): void {
                let mut owner = seed;
                let text = choose(false, &owner, fallback);
                owner[1] = "blocked";
                let _ = text;
            }
            function keepsFallback(seed: [bool, string], fallback: string): void {
                let mut owner = seed;
                let text = view(&mut owner, &fallback);
                dispose(fallback);
                let _ = text;
            }
            function escapedFallback(value: &mut [bool, string]): &string {
                let fallback = "local";
                return view(value, &fallback);
            }
            function keepsExclusive(seed: [bool, string], fallback: &string): bool {
                let mut owner = seed;
                let text = view(&mut owner, fallback);
                let bit = owner[0];
                let _ = text;
                return bit;
            }
            function laterShared(seed: [bool, string]): void {
                let mut owner = seed;
                let _ = view(&mut owner, &owner[1]);
            }
            function laterExclusive(seed: [bool, string]): void {
                let mut owner = seed;
                let _ = reversed(&owner[1], &mut owner);
            }
            function bad(value: &[bool, string], fallback: &string): &string {
                let local = "local";
                return &local;
            }
            function reachedBad(value: &[bool, string], fallback: &string): &string { return bad(value, fallback); }
            function mutableExtra(value: &[bool, string], extra: &mut bool): bool { return false; }
            function unsupportedMutable(value: &[bool, string], extra: &mut bool): bool { return mutableExtra(value, extra); }
            function secondTuple(value: &[bool, string], other: [bool, bool]): bool { return false; }
            function unsupportedSecond(value: &[bool, string], other: [bool, bool]): bool { return secondTuple(value, other); }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let check = |name| lower(name, AnalysisLimits::default());
            let forward = check("forward");
            let mir = forward
                .mir()
                .unwrap_or_else(|| panic!("forward: {:?}", forward.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::Call(_) | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Copy { from: value, into: Some(first) },
                Operation::Copy { from: flag, into: Some(second) },
                Operation::Copy { from: fallback, into: Some(third) },
                Operation::Call(callee), Operation::Export(result),
            ] if value.root.span.bytes() == b"value: &[[bool, string], bool]" && first.root.span.bytes() == b"value"
                && flag.root.span.bytes() == b"flag: bool" && second.root.span.bytes() == b"flag"
                && fallback.root.span.bytes() == b"fallback: &string" && third.root.span.bytes() == b"fallback"
                && **callee == declaration("nestedChoice").id && result.root.span.bytes() == b"nestedChoice(value, flag, fallback)"),
                "{route:?}"
            );
            let limits = AnalysisLimits {
                max_work: forward.statistics().work,
                max_entries: forward.statistics().entries,
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
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            let resumes = check("resumes");
            let mir = resumes
                .mir()
                .expect("scalar and reference results release their loans at different points");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Borrow { .. }
                    | Operation::Call(_)
                    | Operation::Drop(_)
                    | Operation::Install { .. }
                    | Operation::Dispose { .. } => true,
                    Operation::Copy { from, .. } => from.root.span.bytes() == b"text",
                    _ => false,
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Borrow { source: owner, exclusive: true, .. },
                Operation::Borrow { source: first_leaf, exclusive: false, .. },
                Operation::Call(scalar), Operation::Drop(first), Operation::Install { target: staged, .. },
                Operation::Borrow { source: same_owner, exclusive: false, .. },
                Operation::Borrow { source: second_leaf, exclusive: false, .. },
                Operation::Call(choose), Operation::Copy { from: text, .. },
                Operation::Drop(second), Operation::Install { target: replaced, .. },
                Operation::Dispose { operand, .. },
            ] if owner.projection.is_none() && same_owner.projection.is_none() && owner.root.node == same_owner.root.node
                && [first, staged, second, replaced].iter().all(|place| place.root.node == owner.root.node && place.tuple_element() == Some(1))
                && first_leaf.root.span.bytes() == b"fallback: string" && first_leaf.root.node == second_leaf.root.node
                && first_leaf.root.node == operand.root.node && text.root.span.bytes() == b"text"
                && **scalar == declaration("scalar").id && **choose == declaration("choose").id),
                "{route:?}"
            );
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(first), Obligation::Drop(second), Obligation::Drop(last)]
                if first.root.node == second.root.node && second.root.node == last.root.node && first.root.span.bytes() == b"owner"
                    && [first, second, last].iter().all(|place| place.tuple_element() == Some(1)))
            );
            for (name, code, site, related) in [
                ("keepsTuple", "R0203", "owner[1] = \"blocked\";", "&owner"),
                ("keepsFallback", "R0203", "dispose(fallback)", "&fallback"),
                (
                    "escapedFallback",
                    "R0202",
                    "return view(value, &fallback);",
                    "fallback",
                ),
                ("keepsExclusive", "R0201", "owner[0]", "&mut owner"),
                ("laterShared", "R0201", "&owner[1]", "&mut owner"),
                ("laterExclusive", "R0201", "&mut owner", "&owner[1]"),
                ("reachedBad", "R0202", "return &local;", "local"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for name in ["unsupportedMutable", "unsupportedSecond"] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    b"value",
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn direct_calls_with_two_tuple_descriptors_keep_both_restrictions() {
    probe(
        "tuples::direct_calls_with_two_tuple_descriptors_keep_both_restrictions",
        r#"
            function choose(first: &[bool, string], second: &[bool, string], flag: bool): &string {
                let [_, left] = first;
                let [_, right] = second;
                if (flag) { return left; }
                return right;
            }
            function view(first: &mut [bool, string], second: &mut [bool, string]): &string {
                let [_, text] = &mut *first;
                return &*text;
            }
            function snapshot(first: &mut [bool, string], second: &mut [bool, string]): bool {
                let [bit, _] = &mut *first;
                return bit;
            }
            function sharedSame(seed: [bool, string]): void {
                let mut owner = seed;
                let text = choose(&owner, &owner, true);
                let _ = text;
                owner[1] = "new";
            }
            function distinct(left: [bool, string]): bool {
                let mut first = left;
                let mut second: [bool, string] = [true, "initial"];
                let bit = snapshot(&mut first, &mut second);
                first[1] = "staged";
                second[1] = "staged";
                let text = view(&mut first, &mut second);
                let _ = text;
                first[1] = "new";
                second[1] = "new";
                return bit;
            }
            function mixedShape(first: &[[bool, string], bool], second: &[bool, string], leaf: &string): &string {
                let [[_, text], _] = first;
                return text;
            }
            function nestedAndFlat(first: &[[bool, string], bool], second: &[bool, string], leaf: &string): &string {
                return mixedShape(first, second, leaf);
            }
            function narrow(first: &[bool, string], second: &[bool, bool]): bool { return true; }
            function wide(first: &[bool, string], second: &[bool, bool, bool, bool]): bool { return true; }
            function callNarrowSecond(first: &[bool, string], second: &[bool, bool]): bool { return narrow(first, second); }
            function callWideSecond(first: &[bool, string], second: &[bool, bool, bool, bool]): bool { return wide(first, second); }
            function resumesParents(first: &mut [bool, string], second: &mut [bool, string]): bool {
                let bit = snapshot(first, second);
                let text = view(first, second);
                let _ = text;
                let _ = snapshot(first, second);
                return bit;
            }
            function firstSuspended(first: &mut [bool, string], second: &mut [bool, string]): void {
                let text = view(first, second);
                let [bit, _] = &mut *first;
                let _ = text;
            }
            function secondSuspended(first: &mut [bool, string], second: &mut [bool, string]): void {
                let text = view(first, second);
                let [bit, _] = &mut *second;
                let _ = text;
            }
            function keepsFirst(left: [bool, string], right: [bool, string]): void {
                let mut first = left;
                let second = right;
                let text = choose(&first, &second, false);
                first[1] = "blocked";
                let _ = text;
            }
            function keepsSecond(left: [bool, string], right: [bool, string]): void {
                let mut first = left;
                let mut second = right;
                let text = choose(&first, &second, true);
                second[1] = "blocked";
                let _ = text;
            }
            function secondStillExclusive(left: [bool, string], right: [bool, string]): bool {
                let mut first = left;
                let mut second = right;
                let text = view(&mut first, &mut second);
                let bit = second[0];
                let _ = text;
                return bit;
            }
            function freshConflict(seed: [bool, string]): void {
                let mut owner = seed;
                let _ = view(&mut owner, &mut (owner));
            }
            function parentConflict(seed: [bool, string]): void {
                let mut owner = seed;
                let parent = &mut owner;
                let _ = view(parent, (parent));
            }
            function mixedConflict(first: &mut [bool, string], second: &[bool, string]): &string {
                let [_, text] = second;
                return text;
            }
            function sharedAfterExclusive(seed: [bool, string]): void {
                let mut owner = seed;
                let _ = mixedConflict(&mut owner, &owner);
            }
            function unsupportedLeaf(value: &[bool, string], other: &[&bool, string]): bool { return true; }
            function unsupportedSecond(value: &[bool, string], other: &[&bool, string]): bool { return unsupportedLeaf(value, other); }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let check = |name| lower(name, AnalysisLimits::default());
            let shared = check("sharedSame");
            let mir = shared
                .mir()
                .unwrap_or_else(|| panic!("sharedSame: {:?}", shared.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(first), Obligation::Drop(last)]
                if first.root.node == last.root.node && first.root.span.bytes() == b"owner"
                    && first.tuple_element() == Some(1) && last.tuple_element() == Some(1))
            );
            let distinct = check("distinct");
            let mir = distinct
                .mir()
                .expect("both disjoint owners are usable after their last loan use");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Borrow { .. }
                    | Operation::Call(_)
                    | Operation::Drop(_)
                    | Operation::Install { .. } => true,
                    Operation::Copy { from, .. } => from.root.span.bytes() == b"text",
                    _ => false,
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Borrow { source: first, exclusive: true, .. },
                Operation::Borrow { source: second, exclusive: true, .. }, Operation::Call(snapshot),
                Operation::Drop(old_first), Operation::Install { target: staged_first, .. },
                Operation::Drop(old_second), Operation::Install { target: staged_second, .. },
                Operation::Borrow { source: first_again, exclusive: true, .. },
                Operation::Borrow { source: second_again, exclusive: true, .. }, Operation::Call(view),
                Operation::Copy { from: text, .. },
                Operation::Drop(first_staged), Operation::Install { target: replaced_first, .. },
                Operation::Drop(second_staged), Operation::Install { target: replaced_second, .. },
            ] if first.root.span.bytes() == b"first" && second.root.span.bytes() == b"second"
                && first.projection.is_none() && second.projection.is_none()
                && first_again.root.node == first.root.node && second_again.root.node == second.root.node
                && [old_first, staged_first, first_staged, replaced_first].iter().all(|p| p.root.node == first.root.node && p.tuple_element() == Some(1))
                && [old_second, staged_second, second_staged, replaced_second].iter().all(|p| p.root.node == second.root.node && p.tuple_element() == Some(1))
                && text.root.span.bytes() == b"text" && **snapshot == declaration("snapshot").id && **view == declaration("view").id),
                "{route:?}"
            );
            let drops: Vec<_> = path_drops(mir, mir.entry)
                .into_iter()
                .map(|cleanup| match cleanup {
                    Obligation::Drop(place) => (place.root.span.bytes(), place.tuple_element()),
                    other => panic!("unexpected cleanup: {other:?}"),
                })
                .collect();
            assert_eq!(
                drops,
                [
                    (b"first".as_slice(), Some(1)),
                    (b"second".as_slice(), Some(1)),
                    (b"first".as_slice(), Some(1)),
                    (b"second".as_slice(), Some(1)),
                    (b"second".as_slice(), Some(1)),
                    (b"first".as_slice(), Some(1))
                ]
            );
            let limits = AnalysisLimits {
                max_work: distinct.statistics().work,
                max_entries: distinct.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("distinct", limits).mir().is_some());
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
                let limited = lower("distinct", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            let mixed = check("nestedAndFlat");
            let mir = mixed
                .mir()
                .expect("two admitted shared shapes can be passed together");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::Call(_) | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Copy { from: first, into: Some(first_arg) }, Operation::Copy { from: second, into: Some(second_arg) },
                Operation::Copy { from: leaf, into: Some(third_arg) }, Operation::Call(callee), Operation::Export(output),
            ] if first.root.span.bytes() == b"first: &[[bool, string], bool]" && second.root.span.bytes() == b"second: &[bool, string]"
                && first_arg.root.span.bytes() == b"first" && second_arg.root.span.bytes() == b"second"
                && leaf.root.span.bytes() == b"leaf: &string" && third_arg.root.span.bytes() == b"leaf"
                && **callee == declaration("mixedShape").id && output.root.span.bytes() == b"mixedShape(first, second, leaf)"),
                "{route:?}"
            );
            let narrow = check("callNarrowSecond");
            let wide = check("callWideSecond");
            assert!(narrow.mir().is_some() && wide.mir().is_some());
            // Two more leaves in only the second descriptor contribute to its scan and comparison.
            assert!(wide.statistics().work >= narrow.statistics().work + 4);
            let resumes = check("resumesParents");
            let mir = resumes
                .mir()
                .expect("both incoming parents resume after scalar calls and last use");
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Reborrow { .. } | Operation::Call(_) => true,
                    Operation::Copy { from, .. } => from.root.span.bytes() == b"text",
                    _ => false,
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [a, b, Operation::Call(snapshot), c, d, Operation::Call(view), Operation::Copy { .. }, e, f, Operation::Call(again)]
                if **snapshot == declaration("snapshot").id && **view == declaration("view").id && snapshot == again
                && [a, c, e].iter().all(|op| matches!(op, Operation::Reborrow { parent, exclusive: true, .. }
                    if parent.root.span.bytes() == b"first: &mut [bool, string]"))
                && [b, d, f].iter().all(|op| matches!(op, Operation::Reborrow { parent, exclusive: true, .. }
                    if parent.root.span.bytes() == b"second: &mut [bool, string]"))),
                "{route:?}"
            );
            for (name, code, site, related) in [
                ("firstSuspended", "R0201", "&mut *first", "first"),
                ("secondSuspended", "R0201", "&mut *second", "second"),
                ("keepsFirst", "R0203", "first[1] = \"blocked\";", "&first"),
                (
                    "keepsSecond",
                    "R0203",
                    "second[1] = \"blocked\";",
                    "&second",
                ),
                ("secondStillExclusive", "R0201", "second[0]", "&mut second"),
                ("freshConflict", "R0201", "&mut (owner)", "&mut owner"),
                ("parentConflict", "R0201", "(parent)", "parent"),
                ("sharedAfterExclusive", "R0201", "&owner", "&mut owner"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            let unsupported = check("unsupportedSecond");
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert!(unsupported.mir().is_none());
            assert_eq!(unsupported.blocked_at().unwrap().span.bytes(), b"value");
        },
    );
}

#[test]
fn direct_pipes_evaluate_the_input_before_its_earlier_parameters() {
    probe(
        "tuples::direct_pipes_evaluate_the_input_before_its_earlier_parameters",
        r#"
            function view(first: &[bool, string]): &string {
                let [_, text] = first;
                return text;
            }
            function pick(first: &[bool, string], second: &mut [bool, string], flag: bool): &string {
                return view(first);
            }
            function snapshot(first: &mut [bool, string], second: &mut [bool, string]): bool {
                let [bit, _] = &mut *first;
                return bit;
            }
            function bare(first: &[bool, string]): &string { return first |> view; }
            function forward(first: &[bool, string], second: &mut [bool, string], flag: bool): &string {
                return second |> pick(first, _, flag);
            }
            function conflict(seed: [bool, string]): void {
                let mut owner = seed;
                let _ = &mut owner |> pick(&owner, _, true);
            }
            function resumes(left: [bool, string], right: [bool, string]): bool {
                let mut first = left;
                let mut second = right;
                let bit = &mut second |> snapshot(&mut first, _);
                first[1] = "new";
                second[1] = "new";
                return bit;
            }
            function keepsInput(left: [bool, string], right: [bool, string]): void {
                let first = left;
                let mut second = right;
                let text = &mut second |> pick(&first, _, true);
                second[1] = "blocked";
                let _ = text;
            }
            function escapes(first: &[bool, string], seed: [bool, string]): &string {
                let mut second = seed;
                return &mut second |> pick(first, _, true);
            }
            function bad(first: &[bool, string]): &string {
                let local = "local";
                return &local;
            }
            function reachedBad(first: &[bool, string]): &string { return first |> bad; }
            function store(destination: &mut &bool, source: &bool, flag: bool): void { *destination = source; }
            function retained(destination: &mut &bool, source: &bool): void { source |> store(destination, _, true); }
            function owned(first: &[bool, string], second: [bool, string]): bool { return false; }
            function ownedArgument(first: &[bool, string], second: [bool, string]): bool { return second |> owned(first, _); }
            function ownedNested(value: [[bool, bool], bool]): bool { return false; }
            function nestedArgument(): bool { return [[true, false], true] |> ownedNested; }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let check = |name| lower(name, AnalysisLimits::default());
            let forward = check("forward");
            let mir = forward
                .mir()
                .unwrap_or_else(|| panic!("forward: {:?}", forward.completion()));
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::Copy { .. }
                            | Operation::Call(_)
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Reborrow { parent, holder, exclusive: true },
                Operation::Copy { from: first, into: Some(arg) },
                Operation::Copy { from: flag, into: Some(flag_arg) }, Operation::Call(callee), Operation::Export(output),
            ] if parent.root.span.bytes() == b"second: &mut [bool, string]" && holder.root.span.bytes() == b"second"
                && first.root.span.bytes() == b"first: &[bool, string]" && arg.root.span.bytes() == b"first"
                && flag.root.span.bytes() == b"flag: bool" && flag_arg.root.span.bytes() == b"flag"
                && **callee == declaration("pick").id && output.root.span.bytes() == b"second |> pick(first, _, flag)"),
                "{route:?}"
            );
            let limits = AnalysisLimits {
                max_work: forward.statistics().work,
                max_entries: forward.statistics().entries,
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
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            let bare = check("bare");
            let mir = bare.mir().expect("a bare step stages its single argument");
            let calls: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::Call(_)))
                .collect();
            assert!(
                matches!(calls.as_slice(), [Operation::Call(callee)] if **callee == declaration("view").id)
            );
            let resumes = check("resumes");
            let mir = resumes
                .mir()
                .expect("the scalar result releases both stages");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. }
                            | Operation::Call(_)
                            | Operation::Drop(_)
                            | Operation::Install { .. }
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Borrow { source: second, exclusive: true, .. },
                Operation::Borrow { source: first, exclusive: true, .. }, Operation::Call(snapshot),
                Operation::Drop(old_first), Operation::Install { target: new_first, .. },
                Operation::Drop(old_second), Operation::Install { target: new_second, .. },
            ] if first.root.span.bytes() == b"first" && second.root.span.bytes() == b"second"
                && [old_first, new_first].iter().all(|p| p.root.node == first.root.node && p.tuple_element() == Some(1))
                && [old_second, new_second].iter().all(|p| p.root.node == second.root.node && p.tuple_element() == Some(1))
                && **snapshot == declaration("snapshot").id),
                "{route:?}"
            );
            for (name, code, site, related) in [
                ("conflict", "R0201", "&owner", "&mut owner"),
                (
                    "keepsInput",
                    "R0203",
                    "second[1] = \"blocked\";",
                    "&mut second",
                ),
                (
                    "escapes",
                    "R0202",
                    "return &mut second |> pick(first, _, true);",
                    "second",
                ),
                ("reachedBad", "R0202", "return &local;", "local"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for (name, site) in [
                ("retained", "source |> store(destination, _, true)"),
                ("ownedArgument", "second |> owned(first, _)"),
                ("nestedArgument", "[[true, false], true] |> ownedNested"),
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
fn tuple_call_invalidation_does_not_reuse_a_stale_boolean_for_result_use() {
    use compiler::analysis::typed::{ResultDiscardKind, TypedCompletion};

    probe_report(
        "tuples::tuple_call_invalidation_does_not_reuse_a_stale_boolean_for_result_use",
        r#"
            function inspect(owner: &mut [bool, string]): void {}
            function conditional(unhandled: Result<bool, i32>.Ok): void {
                let mut owner: [bool, string] = [true, "initial"];
                inspect(&mut owner);
                if (owner[0]) { let _ = unhandled; }
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let [discard] = uses.discards() else {
                panic!("the post-call condition must not prove acknowledgement")
            };
            assert_eq!(discard.code(), "R0103");
            assert_eq!(discard.kind, ResultDiscardKind::ScopeExit);
            assert_eq!(discard.origin.module, &module.key);
            assert_eq!(
                discard.origin.span.bytes(),
                b"unhandled: Result<bool, i32>.Ok"
            );
        },
    );
}

#[test]
fn mutable_tuple_calls_outside_primitive_storage_keep_the_typed_blocker() {
    use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};

    probe_report(
        "tuples::mutable_tuple_calls_outside_primitive_storage_keep_the_typed_blocker",
        r#"
            function inspect(owner: &mut [bool, Result<bool, i32>.Ok]): void {}
            function blocked(input: Result<bool, i32>.Ok): void {
                let mut owner: [bool, Result<bool, i32>.Ok] = [true, input];
                inspect(&mut owner);
                let _ = owner;
            }
            function inspectNested(value: &mut [[bool, string], bool]): void {}
            function nestedBlocked(): void {
                let mut nested: [[bool, string], bool] = [[true, "initial"], false];
                inspectNested(&mut nested);
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            for site in [b"&mut owner".as_slice(), b"&mut nested".as_slice()] {
                assert!(
                    uses.pending()
                        .iter()
                        .any(|pending| pending.blocker == ResultUseBlocker::Storage
                            && pending.site.module == &module.key
                            && pending.site.span.bytes() == site),
                    "{site:?}"
                );
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
