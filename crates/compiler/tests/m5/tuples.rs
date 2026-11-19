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
            function unsupportedAnnotatedPattern(parent: &mut [[bool, string], bool]): bool {
                let [[first, _], bit]: &mut [[bool, string], bool] = &mut *parent;
                let _ = first;
                return bit;
            }
            function unsupportedSharedReborrow(parent: &mut [bool, string]): void {
                let child = &*parent;
                let _ = child;
            }
            function twoArgs(parent: &mut [bool, string], bit: bool): bool { return bit; }
            function unsupportedSecondArg(seed: [bool, string]): bool {
                let mut owner = seed;
                return twoArgs(&mut owner, true);
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
                ("unsupportedAnnotatedPattern", "[[first, _], bit]"),
                ("unsupportedSecondArg", "&mut owner"),
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
            function exclusive(value: &mut [[bool, string], bool]): bool { return false; }
            function unsupportedExclusive(value: &mut [[bool, string], bool]): bool { return exclusive(value); }
            function compoundResult(value: &[[bool, string], bool]): [bool, bool] { return [false, true]; }
            function unsupportedResult(value: &[[bool, string], bool]): [bool, bool] { return compoundResult(value); }
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
            let scoped = check("forwardsAlias");
            let mir = scoped
                .mir()
                .expect("the returned view does not borrow the local descriptor slot");
            let ops = linear(mir, mir.entry);
            let ended = ops.iter().position(|op| matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action|
                matches!(action, Obligation::EndStorage(place) if place.root.span.bytes() == b"alias")))).unwrap();
            assert!(ops[..ended].iter().any(|op| matches!(op, Operation::Call(callee) if **callee == declaration("nestedView").id)));
            assert!(
                matches!(ops[ended + 1..].iter().copied().filter(|op| matches!(op, Operation::Copy { .. } | Operation::Export(_))).collect::<Vec<_>>().as_slice(), [
                Operation::Copy { from, into: Some(result) }, Operation::Export(out)
            ] if from.root.span.bytes() == b"out" && result.root.node == out.root.node)
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
                "unsupportedExclusive",
                "unsupportedResult",
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
