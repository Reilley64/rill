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
fn option_pipe_propagates_the_staged_call_output_once() {
    probe_with_typed(
        "propagation::option_pipe_propagates_the_staged_call_output_once",
        r#"
            function pass(value: Option<bool>, taken: string): Option<bool> { dispose(taken); return value; }
            function paths(input: Option<bool>, taken: string, owner: string): Option<bool> {
                let bit = taken |> pass(input, _)?;
                dispose(owner);
                return Option.Some(bit);
            }
            function absent(taken: string): Option<bool>.None { dispose(taken); return Option<bool>.None; }
            function failure(taken: string, owner: string): Option<bool> {
                let _ = taken |> absent?;
                dispose(owner);
                return Option.Some(true);
            }
            function present(taken: string): Option<bool>.Some { dispose(taken); return Option.Some(true); }
            function success(taken: string, owner: string): Option<bool> {
                let ((bit)) = (taken |> present?);
                dispose(owner);
                return Option.Some(bit);
            }
            function moved(input: Option<bool>, taken: string): Option<bool> {
                let _ = taken |> pass(input, _)?;
                dispose(taken);
                return Option.Some(true);
            }
            function bad(taken: string): Option<bool>.None {
                dispose(taken);
                dispose(taken);
                return Option<bool>.None;
            }
            function reachedBad(taken: string): Option<bool> {
                let _ = taken |> bad?;
                return Option.Some(true);
            }
            function passText(value: string): Option<string> { return Option.Some(value); }
            function unsupported(input: string): Option<bool> {
                let _ = input |> passText?;
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
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
            let role = typed.option_role().unwrap();
            let report = check("paths");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("paths: {:?}", report.completion()));
            let route: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::Copy { .. }
                            | Operation::Call(_)
                            | Operation::TestVariant { .. }
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::Move { from: owner, into: Some(owner_stage) },
                Operation::Copy { from: input, into: Some(input_stage) },
                Operation::Call(pass), Operation::TestVariant { operand, success },
            ] if owner.root.span.bytes() == b"taken: string" && owner_stage.root.span.bytes() == b"taken"
                && input.root.span.bytes() == b"input: Option<bool>" && input_stage.root.span.bytes() == b"input"
                && *pass == &declaration("pass").id && *success == &role.some.id
                && operand.root.span.bytes() == b"taken |> pass(input, _)?"),
                "{route:?}"
            );
            let (some, none) = branch_targets(mir);
            let success = linear(mir, some);
            let failure = linear(mir, none);
            for branch in [true, false] {
                assert_eq!(calls_on_path(mir, [branch]), [&declaration("pass").id]);
            }
            assert!(success.iter().any(|op| matches!(op, Operation::CopyOptionPayload { operand, into }
                if operand.root.span.bytes() == b"taken |> pass(input, _)?" && into.root.span.bytes() == b"bit")));
            assert!(
                success
                    .iter()
                    .any(|op| matches!(op, Operation::Dispose { operand, .. }
                if operand.root.span.bytes() == b"owner: string"))
            );
            assert_return(&success, &role.some.id, true, &[]);
            assert_return(&failure, &role.none.id, false, &[b"owner: string"]);
            assert!(!failure.iter().any(|op| matches!(
                op,
                Operation::CopyOptionPayload { .. } | Operation::Dispose { .. }
            )));
            for (name, callee, variant, has_payload, drops) in [
                (
                    "failure",
                    "absent",
                    &role.none.id,
                    false,
                    &[b"owner: string".as_slice()][..],
                ),
                ("success", "present", &role.some.id, true, &[][..]),
            ] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                assert_eq!(calls_on_path(mir, []), [&declaration(callee).id]);
                let ops = linear(mir, mir.entry);
                assert_return(&ops, variant, has_payload, drops);
                assert_eq!(
                    ops.iter()
                        .any(|op| matches!(op, Operation::CopyOptionPayload { .. })),
                    has_payload
                );
            }
            for (name, related) in [("moved", "taken"), ("reachedBad", "dispose(taken)")] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one moved-source diagnostic")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), b"dispose(taken)");
                assert_eq!(finding.related.span.bytes(), related.as_bytes());
            }
            let unsupported = check("unsupported");
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert_eq!(
                unsupported.blocked_at().unwrap().span.bytes(),
                b"input |> passText?"
            );
            assert!(unsupported.mir().is_none());
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("paths", limits).mir().is_some());
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
                let limited = lower("paths", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn scalar_infix_wraps_only_failure_and_preserves_the_cause() {
    probe_with_typed(
        "propagation::scalar_infix_wraps_only_failure_and_preserves_the_cause",
        r#"
            function left(input: Result<bool, i32>): Result<bool, i32> { return input; }
            function outer(message: string): i32 { dispose(message); return 42i32; }
            function paths(input: Result<bool, i32>, message: string, owner: string): Result<bool, i32> {
                let bit = left(input) ? outer(message);
                return Result.Ok(bit);
            }
            function pair(message: string): [string, i32] { return [message, 9i32]; }
            function projected(input: Result<i32, i32>, message: string, owner: string): Result<i32, i32> {
                let number = input ? pair(message)[1];
                return Result.Ok(number);
            }
            function badOuter(message: string): i32 {
                dispose(message);
                dispose(message);
                return 5i32;
            }
            function reachesBad(input: Result<bool, i32>, message: string): Result<bool, i32> {
                let bit = input ? badOuter(message);
                return Result.Ok(bit);
            }
            function skipsBad(input: Result<bool, i32>.Ok, message: string): Result<bool, i32> {
                let bit = input ? badOuter(message);
                return Result.Ok(bit);
            }
            function knownOk(input: Result<bool, i32>.Ok, message: string): Result<i32, i32> {
                dispose(message);
                let _ = (input ? outer(message));
                return Result.Ok(7i32);
            }
            function knownError(input: Result<bool, i32>.Err, message: string, owner: string): Result<i32, i32> {
                let _ = input ? outer(message);
                return Result.Ok(7i32);
            }
            function movedRhs(input: Result<bool, i32>, message: string): Result<i32, i32> {
                dispose(message);
                let _ = input ? outer(message);
                return Result.Ok(7i32);
            }
            function movedLeft(input: Result<bool, i32>): Result<bool, i32> {
                let _ = input ? 42i32;
                return input;
            }
            function otherType(input: Result<bool, i32>, message: string): Result<bool, string> {
                let bit = input ? message;
                return Result.Ok(bit);
            }
        "#,
        |_, module, receipt, typed| {
            use compiler::analysis::scalars::{NodeFactKind, data::DataExpression};

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
            let role = typed.result_role().unwrap();
            let projected = lower("projected");
            assert_eq!(
                projected.completion(),
                Completion::Checked,
                "stage RHS temporary projection"
            );
            let report = lower("paths");
            let mir = report.mir().expect("both infix routes lower");
            let prefix = straight_blocks(mir, mir.entry);
            let test = prefix.last().unwrap();
            let (
                Operation::TestVariant {
                    operand: staged_left,
                    success,
                },
                Terminator::Branch { yes, no },
            ) = (&test.operation, test.terminator)
            else {
                panic!("expected to test the staged left Result");
            };
            assert_eq!(*success, &role.ok.id);
            let left = prefix
                .iter()
                .find(|b| matches!(b.operation, Operation::Call(_)))
                .unwrap();
            assert_eq!(left.site.span.bytes(), b"left(input)");
            assert_eq!(staged_left.root.node, left.site.node);
            assert_eq!(calls_on_path(mir, [true]), [&declaration("left").id]);
            assert_eq!(
                calls_on_path(mir, [false]),
                [&declaration("left").id, &declaration("outer").id]
            );
            let success = linear(mir, yes);
            assert!(!success.iter().any(|op| matches!(
                op,
                Operation::WrapResultFailure { .. } | Operation::ForwardResultFailure { .. }
            )));
            let success_transfers: Vec<_> = success
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::TakeResultSuccess { .. }
                            | Operation::Copy { .. }
                            | Operation::ConstructVariant { .. }
                    )
                })
                .collect();
            assert!(matches!(success_transfers.as_slice(), [
                Operation::TakeResultSuccess { operand, into: Some(bit) },
                Operation::Copy { from, into: Some(payload) },
                Operation::ConstructVariant { payload: Some(used), .. },
            ] if operand.root.node == staged_left.root.node && bit.root.span.bytes() == b"bit"
                && from.root.node == bit.root.node && payload.root.node == used.root.node));
            assert_return(
                &success,
                &role.ok.id,
                true,
                &[b"owner: string", b"message: string"],
            );

            let failure = straight_blocks(mir, no);
            assert!(matches!(
                failure.last().unwrap().terminator,
                Terminator::Return
            ));
            let route: Vec<_> = failure
                .iter()
                .copied()
                .filter(|b| {
                    matches!(
                        b.operation,
                        Operation::Move { .. }
                            | Operation::Call(_)
                            | Operation::WrapResultFailure { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            let [transfer, rhs, wrap, export] = route.as_slice() else {
                panic!("expected to move the RHS argument, call, wrap and export");
            };
            assert!(
                matches!(&transfer.operation, Operation::Move { from, into: Some(arg) }
                if from.root.span.bytes() == b"message: string" && arg.root.span.bytes() == b"message"
                    && arg.root.span.start() > rhs.site.span.start() && arg.root.span.end() < rhs.site.span.end())
            );
            assert!(matches!(export.operation, Operation::Export(_)));
            let Operation::WrapResultFailure {
                operand,
                payload,
                into,
                variant,
                wrapper,
                origin,
                cause_display,
            } = &wrap.operation
            else {
                panic!("expected to wrap the staged failure")
            };
            assert_eq!(rhs.site.span.bytes(), b"outer(message)");
            assert_eq!(rhs.site.node, payload.root.node);
            assert_eq!(staged_left.root.node, operand.root.node);
            assert_eq!(*variant, &role.err.id);
            assert_eq!(*wrapper, &role.wrapper.id);
            let source = typed
                .bodies()
                .node(module, wrap.site.node)
                .unwrap()
                .unwrap();
            let NodeFactKind::Expression(facts) = &source.kind else {
                panic!("expected infix facts")
            };
            let Some(DataExpression::Wrapping(data)) = &facts.data else {
                panic!("expected typed wrapping")
            };
            assert_eq!(*origin, &data.construction.origin);
            assert_eq!(origin.snapshot(), &module.snapshot);
            assert_eq!(
                origin.start(),
                wrap.site.span.start() + b"left(input) ".len()
            );
            assert_eq!(origin.bytes(), b"?");
            let retained = data.construction.retained.as_ref().unwrap();
            assert_eq!(
                *cause_display,
                typed
                    .bodies()
                    .goal(&retained.display)
                    .unwrap()
                    .selection
                    .as_ref()
                    .unwrap()
                    .implementation
            );
            let failure_ops: Vec<_> = failure.iter().map(|b| &b.operation).collect();
            assert!(!failure_ops.iter().any(|op| matches!(
                op,
                Operation::ConstructResultError { .. }
                    | Operation::ForwardResultFailure { .. }
                    | Operation::TakeResultSuccess { .. }
            )));
            assert_export(&failure_ops, &[b"owner: string"], |op, output| {
                matches!(op,
                    Operation::WrapResultFailure { into: emitted, .. }
                        if emitted.root.node == into.root.node && into.root.node == output.root.node
                )
            });

            let projected = projected.mir().unwrap();
            let (ok, err) = branch_targets(projected);
            assert!(calls_on_path(projected, [true]).is_empty());
            assert_eq!(calls_on_path(projected, [false]), [&declaration("pair").id]);
            assert_return(
                &linear(projected, ok),
                &role.ok.id,
                true,
                &[b"owner: string", b"message: string"],
            );
            let mut stages = Vec::new();
            for block in straight_blocks(projected, err) {
                match &block.operation {
                    Operation::Call(_) => stages.push(("call", block.site.node, None)),
                    Operation::Copy {
                        from,
                        into: Some(staged),
                    } if from.tuple_element() == Some(1) => {
                        stages.push(("project", from.root.node, Some(staged.root.node)));
                    }
                    Operation::WrapResultFailure { payload, into, .. } => {
                        stages.push(("wrap", payload.root.node, Some(into.root.node)));
                    }
                    Operation::Export(output) => stages.push(("export", output.root.node, None)),
                    Operation::Cleanup(cleanup) => {
                        for action in cleanup {
                            if let Obligation::Drop(place) = action {
                                let label = if place.tuple_element() == Some(0)
                                    && place.root.span.bytes() == b"pair(message)"
                                {
                                    "temporary"
                                } else if place.root.span.bytes() == b"owner: string" {
                                    "owner"
                                } else {
                                    panic!("unexpected drop {place:?}");
                                };
                                stages.push((label, place.root.node, None));
                            }
                        }
                    }
                    _ => {}
                }
            }
            assert!(matches!(stages.as_slice(), [
                ("call", call, None),
                ("project", tuple, Some(staged)),
                ("wrap", payload, Some(result)),
                ("temporary", sibling, None),
                ("export", exported, None),
                ("owner", _, None),
            ] if call == tuple && tuple == sibling && staged == payload && result == exported));

            let known = lower("knownOk");
            let mir = known.mir().expect("known success skips the moved RHS");
            let ops = linear(mir, mir.entry);
            assert!(
                ops.iter()
                    .any(|op| matches!(op, Operation::TakeResultSuccess { into: None, .. }))
            );
            assert!(
                !ops.iter().any(|op| matches!(
                    op,
                    Operation::Call(_) | Operation::WrapResultFailure { .. }
                ))
            );
            assert_return(&ops, &role.ok.id, true, &[]);

            let known = lower("knownError");
            let mir = known.mir().expect("known failure wraps and returns");
            let ops = linear(mir, mir.entry);
            assert_eq!(calls_on_path(mir, []), [&declaration("outer").id]);
            assert_export(&ops, &[b"owner: string"], |op, output| {
                matches!(op,
                    Operation::WrapResultFailure { operand, into, .. }
                        if operand.root.span.bytes() == b"input" && into.root.node == output.root.node
                )
            });

            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("paths", limits).completion(), Completion::Checked);
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
                let short = check("paths", short);
                assert_eq!(short.completion(), Completion::ResourceLimit);
                assert!(short.mir().is_none());
            }
            for (name, site, related) in [
                ("movedRhs", "message", "dispose(message)"),
                ("movedLeft", "input", "input"),
            ] {
                let moved = lower(name);
                assert_eq!(moved.completion(), Completion::SourceRejected, "{name}");
                assert!(moved.mir().is_none());
                let [finding] = moved.findings() else {
                    panic!("expected one moved operand")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(
                    std::str::from_utf8(finding.site.span.bytes()),
                    Ok(site),
                    "{name} primary"
                );
                assert_eq!(
                    std::str::from_utf8(finding.related.span.bytes()),
                    Ok(related),
                    "{name} related"
                );
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            let bad = lower("reachesBad");
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected the reached RHS rejection")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(message)");
            assert_eq!(finding.related.span.bytes(), b"dispose(message)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let skipped = lower("skipsBad");
            let mir = skipped
                .mir()
                .expect("an infeasible RHS does not reach its callee");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &role.ok.id,
                true,
                &[b"message: string"],
            );
            let unsupported = lower("otherType");
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert!(unsupported.mir().is_none());
        },
    );
}

#[test]
fn fresh_scalar_error_keeps_constructor_origin_through_postfix() {
    probe_with_typed(
        "propagation::fresh_scalar_error_keeps_constructor_origin_through_postfix",
        r#"
            import std::result::Err as fail;
            function payload(message: string): i32 { dispose(message); return 7i32; }
            function direct(message: string, owner: string): Result<bool, i32> {
                return (fail(payload(message)));
            }
            function early(message: string, owner: string): Result<i32, i32> {
                let _ = fail<bool, i32>(payload(message))?;
                dispose(owner);
                return Result.Ok(9i32);
            }
            function moved(message: string): Result<bool, i32> {
                dispose(message);
                return fail(payload(message));
            }
            function discarded(): Result<bool, i32> {
                let _ = fail<bool, i32>(7i32);
                return Result.Ok(true);
            }
            function otherPayload(): Result<bool, string> { return fail("error"); }
        "#,
        |_, module, receipt, typed| {
            use compiler::analysis::scalars::{NodeFactKind, data::DataExpression};

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
            let role = typed.result_role().expect("canonical Result");
            for (name, written_call) in [
                ("direct", b"fail(payload(message))".as_slice()),
                ("early", b"fail<bool, i32>(payload(message))".as_slice()),
            ] {
                let report = lower(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let mut path = straight_blocks(mir, mir.entry);
                if name == "early" {
                    let branch = path.last().unwrap();
                    let (
                        Operation::TestVariant { operand, success },
                        Terminator::Branch { yes, no },
                    ) = (&branch.operation, branch.terminator)
                    else {
                        panic!("expected a Result variant test");
                    };
                    assert_eq!(*success, &role.ok.id);
                    let success = linear(mir, yes);
                    assert!(!success.iter().any(|op| matches!(
                        op,
                        Operation::Call(_)
                            | Operation::ConstructResultError { .. }
                            | Operation::ForwardResultFailure { .. }
                    )));
                    let transfer: Vec<_> = success
                        .iter()
                        .copied()
                        .filter(|op| {
                            matches!(
                                op,
                                Operation::TakeResultSuccess { .. } | Operation::Dispose { .. }
                            )
                        })
                        .collect();
                    assert!(matches!(transfer.as_slice(), [
                        Operation::TakeResultSuccess { operand: source, into: None },
                        Operation::Dispose { operand: owner, .. },
                    ] if source.root.node == operand.root.node && owner.root.span.bytes() == b"owner: string"));
                    assert_return(&success, &role.ok.id, true, &[]);
                    path.extend(straight_blocks(mir, no));
                }
                assert!(matches!(
                    path.last().unwrap().terminator,
                    Terminator::Return
                ));
                let ops: Vec<_> = path.iter().map(|b| &b.operation).collect();
                let route: Vec<_> = path
                    .iter()
                    .copied()
                    .filter(|b| {
                        matches!(
                            b.operation,
                            Operation::Call(_)
                                | Operation::ConstructResultError { .. }
                                | Operation::TestVariant { .. }
                                | Operation::ForwardResultFailure { .. }
                                | Operation::Export(_)
                        )
                    })
                    .collect();
                let [call, construct, exit @ ..] = route.as_slice() else {
                    panic!("{name}: expected a payload call and construction");
                };
                let (
                    Operation::Call(callee),
                    Operation::ConstructResultError {
                        constructor,
                        variant,
                        wrapper,
                        payload,
                        into,
                    },
                ) = (&call.operation, &construct.operation)
                else {
                    panic!("{name}: expected to call the payload function before construction");
                };
                assert_eq!(*callee, &declaration("payload").id);
                assert_eq!(*constructor, &role.constructor.unwrap().id);
                assert_eq!(*variant, &role.err.id);
                assert_eq!(*wrapper, &role.wrapper.id);
                assert_eq!(payload.root.node, call.site.node);
                assert_eq!(call.site.span.bytes(), b"payload(message)");
                assert_eq!(construct.site.span.bytes(), written_call);
                let typed_site = typed
                    .bodies()
                    .node(module, construct.site.node)
                    .unwrap()
                    .unwrap();
                let NodeFactKind::Expression(value) = &typed_site.kind else {
                    panic!("expected expression facts for the constructor");
                };
                let Some(DataExpression::ErrorConstruction(error)) = &value.data else {
                    panic!("expected canonical constructor facts");
                };
                assert_eq!(error.origin.snapshot(), construct.site.span.snapshot());
                assert_eq!(error.origin.start(), construct.site.span.start());
                assert_eq!(error.value, payload.root.node);
                assert!(error.retained.is_none());
                match (name, exit) {
                    ("early", [tested, forward, export]) => {
                        assert!(matches!(
                            (&tested.operation, &forward.operation, &export.operation),
                            (Operation::TestVariant { operand: tested, .. },
                                Operation::ForwardResultFailure { operand, .. }, Operation::Export(_))
                            if tested.root.node == into.root.node && operand.root.node == into.root.node
                        ));
                        assert_forward(&ops, role, written_call, &[b"owner: string"]);
                    }
                    ("direct", [export]) => {
                        assert!(matches!(&export.operation, Operation::Export(output)
                            if output.root.node == into.root.node));
                        assert_export(&ops, &[b"owner: string"], |op, output| {
                            matches!(op,
                            Operation::ConstructResultError { into, .. } if into.root.node == output.root.node)
                        });
                    }
                    _ => panic!("{name}: unexpected error construction/exit route"),
                }
                assert!(!ops.iter().any(|op| matches!(
                    op,
                    Operation::Dispose { .. }
                        | Operation::ConstructVariant { .. }
                        | Operation::TakeResultSuccess { .. }
                )));
                let limits = AnalysisLimits {
                    max_work: report.statistics().work,
                    max_entries: report.statistics().entries,
                    ..AnalysisLimits::default()
                };
                assert_eq!(check(name, limits).completion(), Completion::Checked);
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
                    let limited = check(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none());
                }
            }
            let moved = lower("moved");
            assert_eq!(moved.completion(), Completion::SourceRejected);
            assert!(moved.mir().is_none());
            let [finding] = moved.findings() else {
                panic!("expected one moved payload input")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"message");
            assert_eq!(finding.related.span.bytes(), b"dispose(message)");
            for name in ["discarded", "otherPayload"] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none(), "{name}");
            }
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
                return successValue<i32, i32>(1i32);
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

fn calls_on_path<'g, const N: usize>(
    mir: &Mir<'g>,
    branches: [bool; N],
) -> Vec<&'g loader::DeclarationId> {
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
