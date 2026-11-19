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
            function invoked(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                return job();
            }
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
            for (name, site) in [
                ("computed", "() => [payload(flag), text]"),
                ("invoked", "job()"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    std::str::from_utf8(report.blocked_at().unwrap().span.bytes()),
                    Ok(site),
                    "{name}"
                );
            }
        },
    );
}
