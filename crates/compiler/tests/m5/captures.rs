use super::*;

#[test]
fn stored_pair_captures_transfer_backing_and_cleanup() {
    probe_with_typed(
        "captures::stored_pair_captures_transfer_backing_and_cleanup",
        r#"
            function owned(left: string, right: string, owner: string): void {
                let job: once () => [string, string] = () => [right, left];
                let moved = job;
            }
            function copied(text: string, flag: bool): bool {
                let job: once () => [string, bool] = () => [text, flag];
                return flag;
            }
            function movedInput(left: string, right: string): string {
                let job: once () => [string, string] = () => [left, right];
                return right;
            }
            function escaping(text: string, right: string): once () => [&string, string] {
                return () => [&text, right];
            }
            function escapingSecond(text: string, right: string): once () => [string, &string] {
                return () => [right, &text];
            }
            function replace(first: string, second: string, third: string, fourth: string): void {
                let mut job: once () => [string, string] = () => [first, second];
                job = () => [third, fourth];
            }
            function replaceMoved(first: string, second: string, third: string, fourth: string): void {
                let mut job: once () => [string, string] = () => [first, second];
                let old = job;
                job = () => [third, fourth];
            }
            function retireFirst(first: string, second: string): void {
                let job: () => [&string, &string] = () => [&first, &second];
                dispose(first);
                let _ = job;
            }
            function retireSecond(first: string, second: string): void {
                let job: () => [&string, &string] = () => [&first, &second];
                dispose(second);
                let _ = job;
            }
            function fromParameter(view: &string, text: string): once () => [&string, string] {
                return () => [view, text];
            }
            function sameScalar(flag: bool): bool {
                let job: () => [bool, bool] = () => [flag, flag];
                return flag;
            }
            function sameBorrow(text: string): void {
                let _: () => [&string, &string] = () => [&text, &text];
            }
            function grouped(flag: bool, text: string): bool {
                let job: once () => [bool, string] = () => [(flag), text];
                return flag;
            }
            function singleDiscard(text: string): void { let _ = () => text; }
            function singleReplace(first: string, second: string): void {
                let mut job = () => first;
                job = () => second;
            }
            function computed(text: string, flag: bool): void {
                let job: once () => [i32, string] = () => [payload(flag), text];
            }
            function payload(flag: bool): i32 { return 7i32; }
        "#,
        |_, module, receipt, _| {
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
            let owned = lower("owned");
            let mir = owned
                .mir()
                .unwrap_or_else(|| panic!("owned: {:?}", owned.completion()));
            let ops = linear(mir, mir.entry);
            let route: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::Call(_)
                    )
                })
                .collect();
            let [
                Operation::Move {
                    from: right,
                    into: Some(first),
                },
                Operation::Move {
                    from: left,
                    into: Some(second),
                },
                Operation::FormClosurePair {
                    closure,
                    captures,
                    mode,
                },
                Operation::Move {
                    from: job,
                    into: Some(moved),
                },
            ] = route.as_slice()
            else {
                panic!("expected two capture moves, formation, and environment move")
            };
            assert_eq!(*mode, compiler::analysis::InvocationMode::Once);
            assert_eq!(right.root.span.bytes(), b"right: string");
            assert_eq!(left.root.span.bytes(), b"left: string");
            assert_eq!(captures[0].node, first.root.node);
            assert_eq!(captures[1].node, second.root.node);
            assert_eq!(closure.node, job.root.node);
            assert_eq!(moved.root.span.bytes(), b"moved");
            let drops = path_drops(mir, mir.entry);
            assert!(matches!(drops.as_slice(), [
                Obligation::DropCapture { closure: holder1, field: last },
                Obligation::DropCapture { closure: holder2, field: first },
                Obligation::Drop(owner),
            ] if holder1.node == moved.root.node && holder2.node == moved.root.node
                && last.node == captures[1].node && first.node == captures[0].node
                && owner.root.span.bytes() == b"owner: string"));

            let copied = lower("copied");
            let mir = copied
                .mir()
                .expect("copying the scalar does not consume the original");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: flag, into: Some(staged) },
                Operation::FormClosurePair { captures, .. },
                Operation::Copy { from: original, into: Some(result) },
                Operation::Export(exported),
            ] if flag.root.span.bytes() == b"flag: bool" && flag.root.node == original.root.node
                && captures[1].node == staged.root.node && result.root.node == exported.root.node));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::DropCapture { field, .. }]
                if field.span.bytes() == b"text")
            );

            let forwarded = lower("fromParameter");
            let mir = forwarded
                .mir()
                .expect("the reference's backing belongs to the caller");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: view, into: Some(staged) },
                Operation::Move { into: Some(text), .. },
                Operation::FormClosurePair { closure, captures, .. },
                Operation::Export(exported),
            ] if view.root.span.bytes() == b"view: &string" && captures[0].node == staged.root.node
                && captures[1].node == text.root.node && closure.node == exported.root.node));
            assert!(path_drops(mir, mir.entry).is_empty());

            let replace = lower("replace");
            let mir = replace
                .mir()
                .expect("replacement follows the environment owner");
            let mut events = Vec::new();
            let mut formed = None;
            for op in linear(mir, mir.entry) {
                match op {
                    Operation::FormClosurePair { closure, .. } => {
                        formed = Some(closure.node);
                        events.push(b"form".as_slice());
                    }
                    Operation::Install { incoming, target } => {
                        assert_eq!(Some(incoming.root.node), formed);
                        assert_eq!(target.root.span.bytes(), b"job");
                        events.push(b"install".as_slice());
                    }
                    Operation::Cleanup(actions) => {
                        events.extend(actions.iter().filter_map(|action| match action {
                            Obligation::DropCapture { closure, field } => {
                                assert_eq!(closure.span.bytes(), b"job");
                                Some(field.span.bytes())
                            }
                            _ => None,
                        }));
                    }
                    _ => {}
                }
            }
            assert_eq!(
                events,
                [
                    b"form".as_slice(),
                    b"form",
                    b"second",
                    b"first",
                    b"install",
                    b"fourth",
                    b"third"
                ]
            );

            let moved = lower("replaceMoved");
            let mir = moved
                .mir()
                .expect("moving the old environment prevents displacement cleanup");
            let drops: Vec<_> = path_drops(mir, mir.entry)
                .into_iter()
                .map(|action| {
                    let Obligation::DropCapture { closure, field } = action else {
                        panic!("expected only capture cleanup")
                    };
                    (closure.span.bytes(), field.span.bytes())
                })
                .collect();
            assert_eq!(
                drops,
                [
                    (b"old".as_slice(), b"second".as_slice()),
                    (b"old", b"first"),
                    (b"job", b"fourth"),
                    (b"job", b"third"),
                ]
            );

            let limits = AnalysisLimits {
                max_work: owned.statistics().work,
                max_entries: owned.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("owned", limits).completion(), Completion::Checked);
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
                let limited = check("owned", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

            for (name, code, at, related) in [
                ("movedInput", "R0200", "right", "right"),
                (
                    "escaping",
                    "R0202",
                    "return () => [&text, right];",
                    "text: string",
                ),
                (
                    "escapingSecond",
                    "R0202",
                    "return () => [right, &text];",
                    "text: string",
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one ownership error")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(
                    std::str::from_utf8(finding.site.span.bytes()),
                    Ok(at),
                    "{name} primary"
                );
                assert_eq!(
                    std::str::from_utf8(finding.related.span.bytes()),
                    Ok(related),
                    "{name} related"
                );
            }
            for (name, site, backing) in [
                (
                    "retireFirst",
                    b"dispose(first)".as_slice(),
                    b"&first".as_slice(),
                ),
                (
                    "retireSecond",
                    b"dispose(second)".as_slice(),
                    b"&second".as_slice(),
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one live backing conflict")
                };
                assert_eq!(finding.code(), "R0203");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), backing);
            }
            let repeated = lower("sameScalar");
            let mir = repeated
                .mir()
                .expect("the same scalar can supply both copyable positions");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::FormClosurePair { .. }
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: first, into: Some(staged1) },
                Operation::Copy { from: second, into: Some(staged2) },
                Operation::FormClosurePair { captures, .. },
                Operation::Copy { from: after, .. },
            ] if first.root.node == second.root.node && second.root.node == after.root.node
                && staged1.root.node == captures[0].node && staged2.root.node == captures[1].node));
            let loan = lower("sameBorrow");
            let mir = loan
                .mir()
                .expect("two shared loans of a stable place are compatible");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. } | Operation::FormClosurePair { .. }
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source: first, holder: first_holder, exclusive: false },
                Operation::Borrow { source: second, holder: second_holder, exclusive: false },
                Operation::FormClosurePair { captures, .. },
            ] if first.root.node == second.root.node && first_holder.root.node == captures[0].node
                && second_holder.root.node == captures[1].node));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                if text.root.span.bytes() == b"text: string")
            );
            let grouped = lower("grouped");
            let mir = grouped
                .mir()
                .expect("grouping does not change the capture acquisition");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: flag, into: Some(staged) },
                Operation::Move { into: Some(text), .. },
                Operation::FormClosurePair { captures, .. },
                Operation::Copy { from: after, .. },
            ] if flag.root.span.bytes() == b"flag: bool" && flag.root.node == after.root.node
                && staged.root.node == captures[0].node && text.root.node == captures[1].node));
            for (name, capture) in [
                ("singleDiscard", b"text".as_slice()),
                ("singleReplace", b"first".as_slice()),
            ] {
                let report = lower(name);
                let mir = report.mir().unwrap();
                let standalone: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter_map(|op| match op {
                        Operation::DropCapture { field, .. } => Some(field.span.bytes()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(standalone, [capture], "{name}");
            }
            let computed = lower("computed");
            assert_eq!(computed.completion(), Completion::Incomplete);
            assert!(computed.mir().is_none());
            assert_eq!(
                computed.blocked_at().unwrap().span.bytes(),
                b"() => [payload(flag), text]"
            );
        },
    );
}

#[test]
fn invokes_a_known_once_pair_and_transfers_its_contents() {
    probe_with_typed(
        "captures::invokes_a_known_once_pair_and_transfers_its_contents",
        r#"
            function direct(left: string, right: string): string {
                let job: once () => [string, string] = () => [right, left];
                let [first, _] = job();
                return first;
            }
            function moved(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let now = job;
                return now();
            }
            function selected(flag: bool, first: string, second: string, third: string, fourth: string): [string, string] {
                let mut job: once () => [string, string] = () => [first, second];
                if (flag) { job = () => [third, fourth]; }
                return job();
            }
            function consumed(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let result = job();
                let old = job;
                return result;
            }
            function repeatable(flag: bool, other: bool): [bool, bool] {
                let job: () => [bool, bool] = () => [flag, other];
                return job();
            }
        "#,
        |_, module, receipt, typed| {
            use compiler::analysis::InvocationMode;
            use compiler::analysis::scalars::data::Access;

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
            let direct = lower("direct");
            let mir = direct
                .mir()
                .unwrap_or_else(|| panic!("direct: {:?}", direct.completion()));
            let blocks = straight_blocks(mir, mir.entry);
            assert!(matches!(
                blocks.last().unwrap().terminator,
                Terminator::Return
            ));
            let route: Vec<_> = blocks
                .iter()
                .copied()
                .filter(|block| match &block.operation {
                    Operation::FormClosurePair { .. } | Operation::InvokeClosurePair { .. } => true,
                    Operation::Move { from, .. } => from.tuple_element().is_some(),
                    _ => false,
                })
                .collect();
            let [form, call, transfer] = route.as_slice() else {
                panic!("expected formation, invocation, and tuple transfer");
            };
            let (
                Operation::FormClosurePair {
                    closure,
                    captures,
                    mode,
                },
                Operation::InvokeClosurePair {
                    receiver,
                    origin,
                    into,
                },
                Operation::Move {
                    from,
                    into: Some(first),
                },
            ) = (&form.operation, &call.operation, &transfer.operation)
            else {
                panic!("expected the direct once invocation route");
            };
            assert_eq!(*mode, InvocationMode::Once);
            assert_eq!(closure.node, receiver.root.node);
            assert_eq!(origin.node, form.site.node);
            assert_eq!(origin.span.bytes(), b"() => [right, left]");
            assert_eq!(captures[0].span.bytes(), b"right");
            assert_eq!(captures[1].span.bytes(), b"left");
            assert_eq!(from.root.node, into.root.node);
            assert_eq!(from.tuple_element(), Some(0));
            assert_eq!(first.root.span.bytes(), b"first");
            assert_eq!(call.site.span.bytes(), b"job()");
            let typed_call = typed
                .bodies()
                .node(module, call.site.node)
                .unwrap()
                .unwrap();
            let invocation = typed_call
                .callable
                .as_ref()
                .unwrap()
                .invocation
                .as_ref()
                .unwrap();
            assert_eq!(invocation.mode, InvocationMode::Once);
            assert_eq!(invocation.access, Access::Owned);
            assert!(invocation.assumption.is_none());
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(unbound)]
                if unbound.root.node == into.root.node && unbound.tuple_element() == Some(1))
            );

            let moved = lower("moved");
            let mir = moved
                .mir()
                .expect("formation identity moves with its environment");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::InvokeClosurePair { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [_, _,
                Operation::FormClosurePair { closure, .. },
                Operation::Move { from, into: Some(moved) },
                Operation::InvokeClosurePair { receiver, into, .. },
                Operation::Export(output),
            ] if from.root.node == closure.node && moved.root.span.bytes() == b"now"
                && moved.root.node == receiver.root.node && into.root.node == output.root.node));
            assert!(path_drops(mir, mir.entry).is_empty());

            let selected = lower("selected");
            let mir = selected
                .mir()
                .expect("each continuing path invokes the origin it stored");
            let (yes, no) = branch_targets(mir);
            for (edge, expected) in [
                (yes, b"() => [third, fourth]".as_slice()),
                (no, b"() => [first, second]".as_slice()),
            ] {
                let ops = linear(mir, edge);
                let route: Vec<_> = ops
                    .iter()
                    .copied()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::InvokeClosurePair { .. } | Operation::Export(_)
                        )
                    })
                    .collect();
                assert!(matches!(route.as_slice(), [
                    Operation::InvokeClosurePair { receiver, origin, into }, Operation::Export(output),
                ] if receiver.root.span.bytes() == b"job" && origin.span.bytes() == expected
                    && into.root.node == output.root.node));
            }
            assert!(matches!(path_drops(mir, yes).as_slice(), [
                Obligation::DropCapture { field: second, .. }, Obligation::DropCapture { field: first, .. },
            ] if second.span.bytes() == b"second" && first.span.bytes() == b"first"));
            assert!(
                matches!(path_drops(mir, no).as_slice(), [Obligation::Drop(fourth), Obligation::Drop(third)]
                if fourth.root.span.bytes() == b"fourth: string" && third.root.span.bytes() == b"third: string")
            );

            let limits = AnalysisLimits {
                max_work: selected.statistics().work,
                max_entries: selected.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("selected", limits).completion(), Completion::Checked);
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
                let report = check("selected", short);
                assert_eq!(report.completion(), Completion::ResourceLimit);
                assert!(report.mir().is_none());
            }
            let consumed = lower("consumed");
            assert_eq!(consumed.completion(), Completion::SourceRejected);
            assert!(consumed.mir().is_none());
            let [finding] = consumed.findings() else {
                panic!("expected use after invocation")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"job");
            assert_eq!(finding.related.span.bytes(), b"job()");
            let repeatable = lower("repeatable");
            assert_eq!(repeatable.completion(), Completion::Incomplete);
            assert!(repeatable.mir().is_none());
            assert_eq!(repeatable.blocked_at().unwrap().span.bytes(), b"job()");
        },
    );
}

#[test]
fn repeated_invocation_and_borrowed_pair_still_need_typed_evidence() {
    use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};

    probe_report(
        "captures::repeated_invocation_and_borrowed_pair_still_need_typed_evidence",
        r#"
            function again(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let _ = job();
                return job();
            }
            function loan(text: string, owner: string): &string {
                let borrowed: once () => [&string, string] = () => [&text, owner];
                let [view, _] = borrowed();
                return view;
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.core_blockers().is_empty());
            assert!(typed.receipt().is_none());
            let pending = typed.result_use().unwrap().pending();
            for written in [b"return job();".as_slice(), b"= borrowed();"] {
                let at = module
                    .snapshot
                    .bytes()
                    .windows(written.len())
                    .position(|w| w == written)
                    .unwrap();
                let skip = written.iter().position(|&b| b == b' ').unwrap() + 1;
                assert!(
                    pending
                        .iter()
                        .any(|p| p.blocker == ResultUseBlocker::CallBoundary
                            && p.site.module == &module.key
                            && p.site.span.start() == at + skip)
                );
            }
        },
    );
}
