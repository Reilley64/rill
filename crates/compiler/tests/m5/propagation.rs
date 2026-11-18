use super::*;

#[test]
fn option_postfix_returns_through_initialized_cleanup() {
    probe(
        "propagation::option_postfix_returns_through_initialized_cleanup",
        r#"
            function pass(value: Option<bool>): Option<bool> { return value; }
            function paths(input: Option<bool>, flag: bool, first: string, second: string): Option<bool> {
                let owner = first;
                if (flag) { dispose(second); }
                let value = pass(input)?;
                dispose(owner);
                return Option.Some(value);
            }
            function rejected(input: Option<bool>, owner: string): Option<bool> {
                let transferred = owner;
                let _ = input?;
                dispose(owner);
                return Option.Some(true);
            }
            function certainFailure(owner: string): Option<bool> {
                let _ = Option<bool>.None?;
                dispose(owner);
                return Option.Some(true);
            }
            function certainSuccess(owner: string): Option<bool> {
                let value = Option.Some(true)?;
                dispose(owner);
                return Option.Some(value);
            }
            function ownedPayload(input: Option<string>): Option<bool> {
                let _ = input?;
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let role = typed.option_role().expect("admitted Option role");
            let report = lower("paths");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("paths: {:?}", report.completion()));
            let (flag_yes, flag_no) = branch_targets(mir);
            for (start, expected) in [
                (flag_yes, &[b"owner".as_slice()][..]),
                (flag_no, &[b"owner".as_slice(), b"second: string"][..]),
            ] {
                let mut test = start;
                let mut calls = 0;
                let (some, none) = loop {
                    match (&mir.blocks[test].operation, mir.blocks[test].terminator) {
                        (
                            Operation::TestOption { operand, some },
                            Terminator::Branch { yes, no },
                        ) => {
                            assert_eq!(operand.root.span.bytes(), b"pass(input)");
                            assert_eq!(*some, &role.some.id);
                            break (yes, no);
                        }
                        (op, Terminator::Goto(next)) => {
                            calls += usize::from(matches!(op, Operation::Call(_)));
                            assert!(
                                !matches!(op, Operation::Drop(place) if place.root.span.bytes() == b"owner")
                            );
                            test = next;
                        }
                        other => panic!("expected option test: {other:?}"),
                    }
                };
                assert_eq!(calls, 1);
                let failure = linear(mir, none);
                assert!(
                    matches!(failure.as_slice(), [Operation::ConstructOption { variant, payload: None, .. }, Operation::Export(_), ..]
                    if *variant == &role.none.id)
                );
                assert_eq!(drops(mir, none), expected);
                assert!(!failure.iter().any(|op| matches!(
                    op,
                    Operation::CopyOptionPayload { .. } | Operation::Dispose { .. }
                )));
                let success = linear(mir, some);
                assert!(
                    matches!(success.as_slice(), [Operation::CopyOptionPayload { into, .. }, Operation::Dispose { operand, .. }, ..]
                    if into.root.span.bytes() == b"value" && operand.root.span.bytes() == b"owner")
                );
                assert!(success.iter().any(|op| matches!(op, Operation::ConstructOption { variant, payload: Some(_), .. } if *variant == &role.some.id)));
                assert_eq!(drops(mir, some), &expected[1..]);
            }

            let bad = lower("rejected");
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected moved owner")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.related.span.bytes(), b"owner");

            for (name, variant) in [
                ("certainFailure", &role.none.id),
                ("certainSuccess", &role.some.id),
            ] {
                let certain = lower(name);
                let mir = certain
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", certain.completion()));
                let ops = linear(mir, mir.entry);
                assert!(ops.iter().any(|op| matches!(op, Operation::ConstructOption { variant: actual, .. } if *actual == variant)));
                if name == "certainFailure" {
                    assert_eq!(drops(mir, mir.entry), [b"owner: string".as_slice()]);
                    assert!(!ops.iter().any(|op| matches!(
                        op,
                        Operation::Dispose { .. } | Operation::CopyOptionPayload { .. }
                    )));
                } else {
                    assert!(drops(mir, mir.entry).is_empty());
                    assert!(
                        ops.iter()
                            .any(|op| matches!(op, Operation::CopyOptionPayload { .. }))
                    );
                }
            }

            let pending = lower("ownedPayload");
            assert_eq!(pending.completion(), Completion::Incomplete);
            assert!(pending.mir().is_none());
        },
    );
}

fn linear<'a, 'g>(mir: &'a Mir<'g>, mut pc: usize) -> Vec<&'a Operation<'g>> {
    let mut ops = Vec::new();
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        if !matches!(block.operation, Operation::None | Operation::Cleanup(_)) {
            ops.push(&block.operation);
        }
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Return => return ops,
            other => panic!("expected linear path, got {other:?}"),
        }
    }
    panic!("path did not reach a return");
}

fn drops<'g>(mir: &Mir<'g>, start: usize) -> Vec<&'g [u8]> {
    path_drops(mir, start)
        .into_iter()
        .map(|op| match op {
            Obligation::Drop(place) => place.root.span.bytes(),
            other => panic!("unexpected cleanup {other:?}"),
        })
        .collect()
}
