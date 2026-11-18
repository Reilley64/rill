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
                let value = Option<bool>.None?;
                dispose(owner);
                return Option.Some(!value);
            }
            function certainSuccess(owner: string): Option<bool> {
                let value = Option.Some(true)?;
                dispose(owner);
                return Option.Some(value);
            }
            function always(): Option<bool>.Some { return Option<bool>.Some(true); }
            function groupedName(owner: string): Option<bool> {
                let ((value)) = always()?;
                dispose(owner);
                return Option.Some(value);
            }
            function groupedDiscard(owner: string): Option<bool> {
                let (_) = pass(Option<bool>.Some(true))?;
                dispose(owner);
                return Option.Some(true);
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
            let pass = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("pass"))
                .unwrap();
            for (flag, start, expected) in [
                (true, flag_yes, &[b"owner".as_slice()][..]),
                (
                    false,
                    flag_no,
                    &[b"owner".as_slice(), b"second: string"][..],
                ),
            ] {
                for success in [true, false] {
                    assert_eq!(calls_on_path(mir, [flag, success]), [&pass.id]);
                }
                let (some, none) = variant_edges(mir, start, &role.some.id);
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
                ("groupedName", &role.some.id),
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

            let discarded = lower("groupedDiscard");
            let mir = discarded
                .mir()
                .unwrap_or_else(|| panic!("groupedDiscard: {:?}", discarded.completion()));
            assert_eq!(
                mir.blocks
                    .iter()
                    .filter(|b| matches!(b.operation, Operation::Call(_)))
                    .count(),
                1
            );
            let (some, none) = branch_targets(mir);
            assert_return(&linear(mir, some), &role.some.id, true, &[]);
            assert_return(
                &linear(mir, none),
                &role.none.id,
                false,
                &[b"owner: string"],
            );
            assert!(
                !mir.blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::CopyOptionPayload { .. }))
            );

            let pending = lower("ownedPayload");
            assert_eq!(pending.completion(), Completion::Incomplete);
            assert!(pending.mir().is_none());
        },
    );
}

#[test]
fn result_postfix_forwards_existing_failure_before_cleanup() {
    probe_with_typed(
        "propagation::result_postfix_forwards_existing_failure_before_cleanup",
        r#"
            import std::result::Ok as successValue;
            function pass(value: Result<bool, i32>): Result<bool, i32> { return value; }
            function paths(input: Result<bool, i32>, flag: bool, first: string, second: string): Result<i32, i32> {
                let owner = first;
                if (flag) { dispose(second); }
                let value = pass(input)?;
                dispose(owner);
                return Result.Ok(7i32);
            }
            function knownError(input: Result<bool, i32>.Err, owner: string): Result<i32, i32> {
                let _ = input?;
                dispose(owner);
                return Result.Ok(9i32);
            }
            function nestedError(input: Result<bool, i32>.Err, owner: string): Result<i32, i32> {
                if (true) { let inner = owner; let _ = input?; }
                return Err(1i32);
            }
            function knownOk(input: Result<bool, i32>.Ok, owner: string): Result<i32, i32> {
                let _ = input?;
                dispose(owner);
                return Result.Ok(9i32);
            }
            function rejected(input: Result<bool, i32>): Result<bool, i32> {
                let _ = input?;
                let _ = input?;
                return Result.Ok(true);
            }
            function ordinaryHelper(): Result<i32, i32> {
                let _ = successValue<bool, i32>(true)?;
                return Result.Ok(7i32);
            }
            function needsDrop(input: Result<bool, i32>): Result<bool, i32> {
                let _ = input;
                return Result.Ok(true);
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
            let role = typed.result_role().expect("admitted Result role");
            let report = lower("paths");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("paths: {:?}", report.completion()));
            let (flag_yes, flag_no) = branch_targets(mir);
            let pass = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("pass"))
                .unwrap();
            for (flag, start, expected) in [
                (true, flag_yes, &[b"owner".as_slice()][..]),
                (
                    false,
                    flag_no,
                    &[b"owner".as_slice(), b"second: string"][..],
                ),
            ] {
                for success in [true, false] {
                    assert_eq!(calls_on_path(mir, [flag, success]), [&pass.id]);
                }
                let (ok, err) = variant_edges(mir, start, &role.ok.id);
                let failure = linear(mir, err);
                assert_forward(&failure, role, b"pass(input)", expected);
                assert!(!failure.iter().any(|op| matches!(
                    op,
                    Operation::Dispose { .. }
                        | Operation::TakeResultSuccess { .. }
                        | Operation::ConstructVariant { .. }
                )));

                let success = linear(mir, ok);
                let transfer: Vec<_> = success
                    .iter()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::TakeResultSuccess { .. } | Operation::Dispose { .. }
                        )
                    })
                    .collect();
                assert!(
                    matches!(transfer.as_slice(), [Operation::TakeResultSuccess { operand, into: Some(into) }, Operation::Dispose { operand: owner, .. }]
                    if operand.root.span.bytes() == b"pass(input)" && into.root.span.bytes() == b"value" && owner.root.span.bytes() == b"owner")
                );
                assert_return(&success, &role.ok.id, true, &expected[1..]);
            }

            let failed = lower("knownError");
            let mir = failed.mir().expect("known Err forwards and cleans");
            let failure = linear(mir, mir.entry);
            assert_forward(&failure, role, b"input", &[b"owner: string"]);
            assert!(!failure.iter().any(|op| matches!(
                op,
                Operation::ConstructVariant { .. } | Operation::Dispose { .. }
            )));
            let nested = lower("nestedError");
            let mir = nested
                .mir()
                .unwrap_or_else(|| panic!("nestedError: {:?}", nested.completion()));
            let failure = linear(mir, mir.entry);
            assert_forward(&failure, role, b"input", &[b"inner"]);
            assert!(
                !failure
                    .iter()
                    .any(|op| matches!(op, Operation::ConstructVariant { .. }))
            );

            let succeeded = lower("knownOk");
            let mir = succeeded.mir().expect("known Ok continues");
            let success = linear(mir, mir.entry);
            assert_return(&success, &role.ok.id, true, &[]);
            assert!(
                success
                    .iter()
                    .any(|op| matches!(op, Operation::TakeResultSuccess { into: None, .. }))
            );

            let bad = lower("rejected");
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected reused Result operand")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"input");
            assert_eq!(finding.related.span.bytes(), b"input");
            assert!(finding.related.span.start() < finding.site.span.start());

            for name in ["ordinaryHelper", "needsDrop"] {
                let pending = lower(name);
                assert_eq!(pending.completion(), Completion::Incomplete, "{name}");
                assert!(pending.mir().is_none());
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
        },
    );
}

fn variant_edges(mir: &Mir<'_>, mut pc: usize, expected: &loader::DeclarationId) -> (usize, usize) {
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        match (&block.operation, block.terminator) {
            (Operation::TestVariant { operand, success }, Terminator::Branch { yes, no }) => {
                assert_eq!(operand.root.span.bytes(), b"pass(input)");
                assert_eq!(*success, expected);
                return (yes, no);
            }
            (_, Terminator::Goto(next)) => pc = next,
            other => panic!("expected reachable variant test: {other:?}"),
        }
    }
    panic!("path did not reach the variant test");
}

fn calls_on_path<'g>(mir: &Mir<'g>, branches: [bool; 2]) -> Vec<&'g loader::DeclarationId> {
    let mut pc = mir.entry;
    let mut choices = branches.into_iter();
    let mut calls = Vec::new();
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        if let Operation::Call(callee) = block.operation {
            calls.push(callee);
        }
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Branch { yes, no } => {
                pc = if choices.next().expect("unexpected branch") {
                    yes
                } else {
                    no
                };
            }
            Terminator::Return => {
                assert!(choices.next().is_none(), "exit skipped a branch");
                return calls;
            }
        }
    }
    panic!("path did not reach a return");
}

fn assert_forward(
    ops: &[&Operation<'_>],
    role: &compiler::analysis::typed::ResultRole<'_>,
    expected_operand: &[u8],
    expected_drops: &[&[u8]],
) {
    assert_export(ops, expected_drops, |op, output| {
        matches!(op, Operation::ForwardResultFailure { operand, into, variant, wrapper }
        if operand.root.span.bytes() == expected_operand && into.root.node == output.root.node
            && *variant == &role.err.id && *wrapper == &role.wrapper.id)
    });
}

fn assert_return(
    ops: &[&Operation<'_>],
    expected: &loader::DeclarationId,
    has_payload: bool,
    expected_drops: &[&[u8]],
) {
    assert_export(ops, expected_drops, |op, output| {
        matches!(op, Operation::ConstructVariant { variant, payload, into }
        if *variant == expected && payload.is_some() == has_payload && into.root.node == output.root.node)
    });
}

fn assert_export<'g>(
    ops: &[&Operation<'g>],
    expected_drops: &[&[u8]],
    defines_output: impl Fn(&Operation<'g>, &compiler::analysis::ownership::Place<'g>) -> bool,
) {
    let (export, output) = ops
        .iter()
        .enumerate()
        .find_map(|(index, op)| match op {
            Operation::Export(place) => Some((index, place)),
            _ => None,
        })
        .expect("export the return value");
    assert_eq!(
        ops[..export]
            .iter()
            .filter(|op| defines_output(op, output))
            .count(),
        1
    );
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
