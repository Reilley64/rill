use super::*;

#[test]
fn option_postfix_returns_through_initialized_cleanup() {
    probe_with_typed(
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
            let check = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let lower = |name| check(name, AnalysisLimits::default());
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
                let (some, none) = (0..mir.blocks.len())
                    .find_map(|_| {
                        match (&mir.blocks[test].operation, mir.blocks[test].terminator) {
                            (
                                Operation::TestOption { operand, some },
                                Terminator::Branch { yes, no },
                            ) => {
                                assert_eq!(operand.root.span.bytes(), b"pass(input)");
                                assert_eq!(*some, &role.some.id);
                                Some((yes, no))
                            }
                            (op, Terminator::Goto(next)) => {
                                calls += usize::from(matches!(op, Operation::Call(_)));
                                test = next;
                                None
                            }
                            other => panic!("expected option test: {other:?}"),
                        }
                    })
                    .expect("path reaches the Option test");
                assert_eq!(calls, 1);
                let failure = linear(mir, none);
                assert_return(&failure, &role.none.id, false, expected);
                assert!(!failure.iter().any(|op| matches!(
                    op,
                    Operation::CopyOptionPayload { .. } | Operation::Dispose { .. }
                )));
                let success = linear(mir, some);
                let transfer: Vec<_> = success
                    .iter()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::CopyOptionPayload { .. } | Operation::Dispose { .. }
                        )
                    })
                    .collect();
                assert!(
                    matches!(transfer.as_slice(), [Operation::CopyOptionPayload { into, .. }, Operation::Dispose { operand, .. }]
                    if into.root.span.bytes() == b"value" && operand.root.span.bytes() == b"owner")
                );
                assert_return(&success, &role.some.id, true, &expected[1..]);
            }
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(check("paths", limits).mir().is_some());
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
                let limited = check("paths", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
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
                if name == "certainFailure" {
                    assert_return(&ops, variant, false, &[b"owner: string"]);
                    assert!(!ops.iter().any(|op| matches!(
                        op,
                        Operation::Dispose { .. } | Operation::CopyOptionPayload { .. }
                    )));
                } else {
                    assert_return(&ops, variant, true, &[]);
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
        ops.push(&block.operation);
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Return => return ops,
            other => panic!("expected linear path, got {other:?}"),
        }
    }
    panic!("path did not reach a return");
}

fn assert_return(
    ops: &[&Operation<'_>],
    expected: &loader::DeclarationId,
    has_payload: bool,
    expected_drops: &[&[u8]],
) {
    let (export, output) = ops
        .iter()
        .enumerate()
        .find_map(|(index, op)| match op {
            Operation::Export(place) => Some((index, place)),
            _ => None,
        })
        .expect("export the return value");
    assert!(ops[..export].iter().any(|op| matches!(op, Operation::ConstructOption { variant, payload, into }
        if *variant == expected && payload.is_some() == has_payload && into.root.node == output.root.node)));
    let mut drops = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        match op {
            Operation::Drop(place) => {
                assert!(index > export, "export the value before exit cleanup");
                drops.push(place.root.span.bytes());
            }
            Operation::Cleanup(cleanup) => {
                for action in cleanup {
                    match action {
                        Obligation::Drop(place) => {
                            assert!(index > export, "export the value before exit cleanup");
                            drops.push(place.root.span.bytes());
                        }
                        Obligation::EndStorage(_) => {}
                        other => panic!("unexpected cleanup {other:?}"),
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(drops, expected_drops);
}
