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
            function widerPayload(input: Option<i32>): Option<bool> {
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
                let (some, none) = variant_edges(mir, start, &role.some.id, b"pass(input)");
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

            let pending = lower("widerPayload");
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
                { let inner = owner; let _ = taken |> absent?; dispose(inner); }
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
            function passNumber(value: string): Option<i32> { dispose(value); return Option.Some(1i32); }
            function unsupported(input: string): Option<bool> {
                let _ = input |> passNumber?;
                return Option.Some(true);
            }
            function nonExact(taken: string): Option<bool> {
                let _ = taken |> pass(Option<bool>.Some(true), _)?;
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
                    &[b"inner".as_slice()][..],
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
            for (name, site) in [
                ("unsupported", "input |> passNumber?"),
                ("nonExact", "taken |> pass(Option<bool>.Some(true), _)?"),
            ] {
                let unsupported = check(name);
                assert_eq!(unsupported.completion(), Completion::Incomplete, "{name}");
                assert_eq!(
                    unsupported.blocked_at().unwrap().span.bytes(),
                    site.as_bytes(),
                    "{name}"
                );
                assert!(unsupported.mir().is_none());
            }
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
                let (ok, err) = variant_edges(mir, start, &role.ok.id, b"pass(input)");
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

#[test]
fn result_pipe_forwards_the_existing_wrapper_after_a_single_call() {
    probe_with_typed(
        "propagation::result_pipe_forwards_the_existing_wrapper_after_a_single_call",
        r#"
            function pass(value: Result<bool, i32>, taken: string): Result<bool, i32> { dispose(taken); return value; }
            function paths(input: Result<bool, i32>, taken: string, owner: string, other: string, flag: bool): Result<i32, i32> {
                if (flag) { dispose(other); }
                let bit = taken |> pass(input, _)?;
                dispose(owner);
                return Result.Ok(4i32);
            }
            function onlyErr(input: Result<bool, i32>.Err): Result<bool, i32>.Err { return input; }
            function knownError(input: Result<bool, i32>.Err, owner: string): Result<i32, i32> {
                let _ = input |> onlyErr?;
                dispose(owner);
                return Result.Ok(6i32);
            }
            function onlyOk(input: Result<bool, i32>.Ok): Result<bool, i32>.Ok { return input; }
            function knownOk(input: Result<bool, i32>.Ok, owner: string): Result<i32, i32> {
                let _ = input |> onlyOk?;
                dispose(owner);
                return Result.Ok(6i32);
            }
            function numberStep(value: i32): Result<i32, i32>.Ok { return Result<i32, i32>.Ok(value); }
            function knownNumber(value: i32, owner: string): Result<bool, i32> {
                let number = value |> numberStep?;
                dispose(owner);
                return Result.Ok(true);
            }
            function text(value: string): Result<string, i32> { return Result.Ok(value); }
            function unsupportedText(value: string): Result<bool, i32> {
                let _ = value |> text?;
                return Result.Ok(true);
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
            let check = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let role = typed.result_role().unwrap();
            let report = check("paths");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("paths: {:?}", report.completion()));
            let (yes, no) = branch_targets(mir);
            let pipe = b"taken |> pass(input, _)?";
            for (flag, start, expected_drops) in [
                (true, yes, &[b"owner: string".as_slice()][..]),
                (
                    false,
                    no,
                    &[b"other: string".as_slice(), b"owner: string"][..],
                ),
            ] {
                for branch in [true, false] {
                    assert_eq!(
                        calls_on_path(mir, [flag, branch]),
                        [&declaration("pass").id]
                    );
                }
                let split = straight_blocks(mir, start);
                let route: Vec<_> = split
                    .iter()
                    .map(|b| &b.operation)
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::Move { .. }
                                | Operation::Call(_)
                                | Operation::TestVariant { .. }
                        )
                    })
                    .collect();
                assert!(
                    matches!(route.as_slice(), [
                    Operation::Move { from: taken, into: Some(taken_stage) },
                    Operation::Move { from: input, into: Some(input_stage) },
                    Operation::Call(pass), Operation::TestVariant { operand, success },
                ] if taken.root.span.bytes() == b"taken: string" && taken_stage.root.span.bytes() == b"taken"
                    && input.root.span.bytes() == b"input: Result<bool, i32>" && input_stage.root.span.bytes() == b"input"
                    && *pass == &declaration("pass").id && *success == &role.ok.id
                    && operand.root.span.bytes() == pipe),
                    "{route:?}"
                );
                let Terminator::Branch { yes, no } = split.last().unwrap().terminator else {
                    panic!("expected Result variant split")
                };
                let err = linear(mir, no);
                assert_forward(&err, role, pipe, expected_drops);
                assert!(!err.iter().any(|op| matches!(
                    op,
                    Operation::ConstructResultError { .. }
                        | Operation::ConstructVariant { .. }
                        | Operation::WrapResultFailure { .. }
                        | Operation::TakeResultSuccess { .. }
                        | Operation::Dispose { .. }
                )));
                let ok = linear(mir, yes);
                assert!(ok.iter().any(
                    |op| matches!(op, Operation::TakeResultSuccess { operand, into: Some(target) }
                    if operand.root.span.bytes() == pipe && target.root.span.bytes() == b"bit")
                ));
                assert!(
                    ok.iter()
                        .any(|op| matches!(op, Operation::Dispose { operand, .. }
                    if operand.root.span.bytes() == b"owner: string"))
                );
                assert_return(
                    &ok,
                    &role.ok.id,
                    true,
                    if flag { &[] } else { &[b"other: string"] },
                );
            }
            let failed = check("knownError");
            let mir = failed
                .mir()
                .expect("the declared Err variant skips the success edge");
            assert_eq!(calls_on_path(mir, []), [&declaration("onlyErr").id]);
            let err = linear(mir, mir.entry);
            assert_forward(&err, role, b"input |> onlyErr?", &[b"owner: string"]);
            assert!(!err.iter().any(|op| matches!(
                op,
                Operation::TakeResultSuccess { .. } | Operation::Dispose { .. }
            )));
            let succeeded = check("knownOk");
            let mir = succeeded
                .mir()
                .expect("the declared Ok variant skips the failure edge");
            assert_eq!(calls_on_path(mir, []), [&declaration("onlyOk").id]);
            let ok = linear(mir, mir.entry);
            assert_return(&ok, &role.ok.id, true, &[]);
            assert!(ok.iter().any(
                |op| matches!(op, Operation::TakeResultSuccess { operand, into: None }
                if operand.root.span.bytes() == b"input |> onlyOk?")
            ));
            let number = check("knownNumber");
            let mir = number
                .mir()
                .expect("an i32 success payload can feed a Bool Result caller");
            assert_eq!(calls_on_path(mir, []), [&declaration("numberStep").id]);
            let ops = linear(mir, mir.entry);
            assert_return(&ops, &role.ok.id, true, &[]);
            assert!(ops.iter().any(|op| matches!(op, Operation::TakeResultSuccess { operand, into: Some(target) }
                if operand.root.span.bytes() == b"value |> numberStep?" && target.root.span.bytes() == b"number")));
            let unsupported = check("unsupportedText");
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert_eq!(
                unsupported.blocked_at().unwrap().span.bytes(),
                b"value |> text?"
            );
            assert!(unsupported.mir().is_none());
        },
    );
}

#[test]
fn fresh_error_pipe_keeps_the_written_constructor_anchor() {
    use compiler::analysis::scalars::{NodeFactKind, data::DataExpression};

    probe_with_typed(
        "propagation::fresh_error_pipe_keeps_the_written_constructor_anchor",
        r#"
            import std::result::Err as makeFailure;
            function payload(taken: string): i32 { dispose(taken); return 17i32; }
            function bad(taken: string): i32 { dispose(taken); dispose(taken); return 17i32; }
            function failed(taken: string, owner: string): Result<i32, i32> {
                let remaining = owner;
                let _ = payload(taken) |> makeFailure<bool, i32>?;
                dispose(remaining);
                return Result.Ok(1i32);
            }
            function moved(taken: string): Result<i32, i32> {
                dispose(taken);
                let _ = payload(taken) |> makeFailure<bool, i32>?;
                return Result.Ok(1i32);
            }
            function reachedBad(taken: string): Result<i32, i32> {
                let _ = bad(taken) |> makeFailure<bool, i32>?;
                return Result.Ok(1i32);
            }
            function placeholder(code: i32, owner: string): Result<bool, i32> {
                let answer = code |> makeFailure<bool, i32>(_)?;
                dispose(owner);
                return Result.Ok(answer);
            }
            function unsupportedPlain(code: i32): Result<bool, i32> { return code |> makeFailure<bool, i32>; }
            function unsupportedText(message: string): Result<bool, string> {
                let _ = message |> makeFailure<bool, string>?;
                return Result.Ok(true);
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
            let report = check("failed");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("failed: {:?}", report.completion()));
            let pipe = b"payload(taken) |> makeFailure<bool, i32>?";
            let role = typed.result_role().unwrap();
            let canonical = &role.providers.unwrap().constructor.id;
            let path = straight_blocks(mir, mir.entry);
            assert!(matches!(
                path.last().unwrap().terminator,
                Terminator::Return
            ));
            let route: Vec<_> = path
                .iter()
                .copied()
                .filter(|b| {
                    matches!(
                        b.operation,
                        Operation::Call(_)
                            | Operation::ConstructResultError { .. }
                            | Operation::ForwardResultFailure { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            let [call, construct, forward, export] = route.as_slice() else {
                panic!("expected one call, construction and export: {route:?}")
            };
            assert!(
                matches!(call.operation, Operation::Call(callee) if callee == &declaration("payload").id)
            );
            assert_eq!(call.site.span.bytes(), b"payload(taken)");
            assert!(
                matches!((&construct.operation, &forward.operation, &export.operation), (
                Operation::ConstructResultError { constructor, variant, wrapper, payload, into },
                Operation::ForwardResultFailure { operand, into: forwarded, variant: failure, wrapper: forwarded_wrapper }, Operation::Export(result),
            ) if *constructor == canonical && *variant == &role.err.id && *failure == *variant && *wrapper == &role.wrapper.id
                && *forwarded_wrapper == *wrapper && payload.root.span.bytes() == b"payload(taken)"
                && into.root.node == operand.root.node && operand.root.span.bytes() == pipe && forwarded.root.node == result.root.node),
                "{route:?}"
            );
            assert_eq!(construct.site.span.bytes(), b"makeFailure<bool, i32>?");
            let typed_pipe = typed
                .bodies()
                .node(module, forward.site.node)
                .unwrap()
                .unwrap();
            let NodeFactKind::Expression(facts) = &typed_pipe.kind else {
                panic!("expected typed pipe")
            };
            let Some(DataExpression::ErrorPipe {
                construction,
                propagation,
            }) = &facts.data
            else {
                panic!("expected typed pipe construction and propagation")
            };
            assert!(propagation.can_return && !propagation.can_continue);
            assert_eq!(
                construction.origin.snapshot(),
                construct.site.span.snapshot()
            );
            assert_eq!(construct.site.span.start(), construction.origin.start());
            assert_eq!(construction.origin.start(), construction.origin.end());
            assert_eq!(construction.value, call.site.node);
            assert!(construction.retained.is_none());
            let ops: Vec<_> = path.iter().map(|block| &block.operation).collect();
            assert_forward(&ops, role, pipe, &[b"remaining"]);
            assert!(!ops.iter().any(|op| matches!(
                op,
                Operation::TakeResultSuccess { .. }
                    | Operation::TestVariant { .. }
                    | Operation::Dispose { .. }
            )));
            let placeholder = check("placeholder");
            let mir = placeholder
                .mir()
                .expect("one placeholder stages the input once");
            assert!(calls_on_path(mir, []).is_empty());
            let ops = linear(mir, mir.entry);
            assert_forward(
                &ops,
                role,
                b"code |> makeFailure<bool, i32>(_)?",
                &[b"owner: string"],
            );
            assert_eq!(
                ops.iter()
                    .filter(
                        |op| matches!(op, Operation::ConstructResultError { payload, .. }
                if payload.root.span.bytes() == b"code")
                    )
                    .count(),
                1
            );
            for (name, site) in [("moved", "taken"), ("reachedBad", "dispose(taken)")] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected exactly one moved-owner finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), site.as_bytes());
                assert_eq!(finding.related.span.bytes(), b"dispose(taken)");
            }
            for (name, site) in [
                ("unsupportedPlain", "code |> makeFailure<bool, i32>"),
                ("unsupportedText", "message |> makeFailure<bool, string>?"),
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
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("failed", limits).mir().is_some());
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
                let limited = lower("failed", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn propagation_statements_discard_only_the_feasible_success_payload() {
    probe_with_typed(
        "propagation::propagation_statements_discard_only_the_feasible_success_payload",
        r#"
            function option(input: Option<bool>, owner: string): Option<bool> {
                ((input?));
                dispose(owner);
                return Option.Some(true);
            }
            function result(input: Result<i32, i32>, owner: string): Result<bool, i32> {
                input?;
                dispose(owner);
                return Result.Ok(true);
            }
            function pass(input: Result<i32, i32>): Result<i32, i32> { return input; }
            function pipe(input: Result<i32, i32>, owner: string): Result<bool, i32> {
                input |> pass?;
                dispose(owner);
                return Result.Ok(true);
            }
            function rhs(message: string): i32 { dispose(message); return 1i32; }
            function wrap(input: Result<i32, i32>, message: string, owner: string): Result<bool, i32> {
                input ? rhs(message);
                return Result.Ok(true);
            }
            function knownFailure(input: Result<i32, i32>.Err, owner: string): Result<bool, i32> {
                input?;
                dispose(owner);
                return Result.Ok(true);
            }
            function embedded(input: Option<bool>): Option<bool> { return Option.Some(input?); }
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
            let option = typed.option_role().unwrap();
            let result = typed.result_role().unwrap();
            let report = check("option");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("option: {:?}", report.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let success = linear(mir, yes);
            let failure = linear(mir, no);
            assert_return(&success, &option.some.id, true, &[]);
            assert!(
                !success
                    .iter()
                    .any(|op| matches!(op, Operation::CopyOptionPayload { .. }))
            );
            assert_return(&failure, &option.none.id, false, &[b"owner: string"]);
            for (name, source) in [
                ("result", b"input".as_slice()),
                ("pipe", b"input |> pass?".as_slice()),
            ] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, source);
                let success = linear(mir, yes);
                let failure = linear(mir, no);
                let transfers: Vec<_> = success
                    .iter()
                    .copied()
                    .filter(|op| matches!(op, Operation::TakeResultSuccess { .. }))
                    .collect();
                assert!(
                    matches!(transfers.as_slice(), [Operation::TakeResultSuccess { operand, into: None }]
                    if operand.root.span.bytes() == source),
                    "{transfers:?}"
                );
                assert_return(&success, &result.ok.id, true, &[]);
                assert_forward(&failure, result, source, &[b"owner: string"]);
                for branch in [true, false] {
                    let calls = calls_on_path(mir, [branch]);
                    if name == "pipe" {
                        assert_eq!(calls, [&declaration("pass").id]);
                    } else {
                        assert!(calls.is_empty());
                    }
                }
            }
            let wrap = check("wrap");
            let mir = wrap
                .mir()
                .expect("infix statement retains the complete failed wrapper");
            let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            let ok = linear(mir, yes);
            let err = linear(mir, no);
            assert!(calls_on_path(mir, [true]).is_empty());
            assert_eq!(calls_on_path(mir, [false]), [&declaration("rhs").id]);
            assert!(
                ok.iter()
                    .any(|op| matches!(op, Operation::TakeResultSuccess { into: None, .. }))
            );
            assert_return(
                &ok,
                &result.ok.id,
                true,
                &[b"owner: string", b"message: string"],
            );
            assert_export(&err, &[b"owner: string"], |op, out| {
                matches!(op, Operation::WrapResultFailure { into, .. }
                if into.root.node == out.root.node)
            });
            let known = check("knownFailure");
            let mir = known
                .mir()
                .expect("a known Err selects only the early exit");
            let err = linear(mir, mir.entry);
            assert_forward(&err, result, b"input", &[b"owner: string"]);
            assert!(!err.iter().any(|op| matches!(
                op,
                Operation::TestVariant { .. }
                    | Operation::Dispose { .. }
                    | Operation::TakeResultSuccess { .. }
            )));
            let embedded = check("embedded");
            assert_eq!(embedded.completion(), Completion::Incomplete);
            assert!(embedded.mir().is_none());
            let limits = AnalysisLimits {
                max_work: wrap.statistics().work,
                max_entries: wrap.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("wrap", limits).mir().is_some());
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
                let limited = lower("wrap", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn result_expression_statements_still_require_acknowledgement() {
    use compiler::analysis::typed::{ResultDiscardKind, TypedCompletion};

    probe_report(
        "propagation::result_expression_statements_still_require_acknowledgement",
        r#"
            function rejected(input: Result<bool, i32>.Ok): void { input; }
        "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let [discard] = uses.discards() else {
                panic!("expected one unacknowledged Result")
            };
            assert_eq!(discard.code(), "R0103");
            assert_eq!(discard.kind, ResultDiscardKind::Expression);
            assert_eq!(discard.discard.span.bytes(), b"input;");
            assert_eq!(discard.origin.span.bytes(), b"input: Result<bool, i32>.Ok");
        },
    );
}

#[test]
fn a_propagating_input_does_not_acknowledge_a_pipe_result() {
    use compiler::analysis::typed::TypedCompletion;

    probe_report(
        "propagation::a_propagating_input_does_not_acknowledge_a_pipe_result",
        r#"
            function step(value: bool): Result<bool, i32> { return Result.Ok(value); }
            function unchecked(input: Result<bool, i32>): Result<bool, i32> {
                (input?) |> step;
                return Result.Ok(true);
            }
        "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
        },
    );
}

#[test]
fn an_early_call_argument_exit_cleans_only_the_staged_prefix() {
    probe_with_typed(
        "propagation::an_early_call_argument_exit_cleans_only_the_staged_prefix",
        r#"
            function before(taken: string): string { return taken; }
            function after(taken: string): string { return taken; }
            function consume(first: string, second: string, bit: bool, last: string): bool {
                dispose(last);
                dispose(second);
                dispose(first);
                return bit;
            }
            function option(input: Option<bool>, first: string, second: string, later: string, owner: string): Option<bool> {
                let remaining = owner;
                let bit = consume(before(first), second, input?, after(later));
                dispose(remaining);
                return Option.Some(bit);
            }
            function result(input: Result<bool, i32>, first: string, second: string, later: string): Result<i32, i32> {
                let _ = consume(first, second, (input?), later);
                return Result.Ok(1i32);
            }
            function bad(first: string, bit: bool): bool { dispose(first); dispose(first); return bit; }
            function reachedBad(input: Option<bool>, first: string): Option<bool> {
                let bit = bad(first, input?);
                return Option.Some(bit);
            }
            function borrowed(first: &string, second: bool): bool { return second; }
            function unsupportedBorrow(input: Option<bool>, owner: string): Option<bool> {
                let bit = borrowed(&owner, input?);
                return Option.Some(bit);
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
            let option = typed.option_role().unwrap();
            let result = typed.result_role().unwrap();
            let report = check("option");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("option: {:?}", report.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_eq!(
                calls_on_path(mir, [true]),
                [
                    &declaration("before").id,
                    &declaration("after").id,
                    &declaration("consume").id
                ]
            );
            assert_eq!(calls_on_path(mir, [false]), [&declaration("before").id]);
            let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| match op {
                    Operation::Move { from, .. } => from.root.span.bytes() == b"second: string",
                    Operation::Call(_) | Operation::TestVariant { .. } => true,
                    _ => false,
                })
                .collect();
            assert!(
                matches!(prefix.as_slice(), [
                Operation::Call(before), Operation::Move { into: Some(second), .. }, Operation::TestVariant { .. },
            ] if *before == &declaration("before").id && second.root.span.bytes() == b"second"),
                "{prefix:?}"
            );
            let ok = linear(mir, yes);
            let none = linear(mir, no);
            assert_return(
                &none,
                &option.none.id,
                false,
                &[b"second", b"before(first)", b"remaining", b"later: string"],
            );
            assert_return(&ok, &option.some.id, true, &[]);
            let route: Vec<_> = ok
                .iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::CopyOptionPayload { .. }
                            | Operation::Call(_)
                            | Operation::Dispose { .. }
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [
                Operation::CopyOptionPayload { operand, into }, Operation::Call(after), Operation::Call(consume), Operation::Dispose { operand: remaining, .. },
            ] if operand.root.span.bytes() == b"input" && into.root.span.bytes() == b"input?" && remaining.root.span.bytes() == b"remaining"
                && *after == &declaration("after").id && *consume == &declaration("consume").id),
                "{route:?}"
            );
            let batches: Vec<_> = none
                .iter()
                .filter_map(|op| match op {
                    Operation::Cleanup(actions) => Some(
                        actions
                            .iter()
                            .filter_map(|action| match action {
                                Obligation::Drop(place) => Some(place.root.span.bytes()),
                                _ => None,
                            })
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .filter(|drops| !drops.is_empty())
                .collect();
            assert_eq!(
                batches,
                [
                    vec![b"second".as_slice(), b"before(first)"],
                    vec![b"remaining".as_slice()],
                    vec![b"later: string".as_slice()]
                ]
            );
            let result_report = check("result");
            let mir = result_report
                .mir()
                .expect("a scalar Result argument forwards the same failure");
            let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            assert_eq!(calls_on_path(mir, [true]), [&declaration("consume").id]);
            assert!(calls_on_path(mir, [false]).is_empty());
            let ok = linear(mir, yes);
            assert!(ok.iter().any(
                |op| matches!(op, Operation::TakeResultSuccess { into: Some(place), .. }
                if place.root.span.bytes() == b"(input?)")
            ));
            assert_return(&ok, &result.ok.id, true, &[]);
            assert_forward(
                &linear(mir, no),
                result,
                b"input",
                &[b"second", b"first", b"later: string"],
            );
            let bad = check("reachedBad");
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected one reached-callee finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(first)");
            assert_eq!(finding.related.span.bytes(), b"dispose(first)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let borrowed = check("unsupportedBorrow");
            assert_eq!(borrowed.completion(), Completion::Incomplete);
            assert!(borrowed.mir().is_none());
            assert_eq!(
                borrowed.blocked_at().unwrap().span.bytes(),
                b"borrowed(&owner, input?)"
            );
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("option", limits).mir().is_some());
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
                let limited = lower("option", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn wrapping_and_pipe_arguments_use_the_same_early_exit_cleanup() {
    probe_with_typed(
        "propagation::wrapping_and_pipe_arguments_use_the_same_early_exit_cleanup",
        r#"
            function firstStage(value: string): string { return value; }
            function lastStage(value: string): string { return value; }
            function consume(first: string, bit: bool, last: string): bool { dispose(last); dispose(first); return bit; }
            function error(message: string): i32 { dispose(message); return 17i32; }
            function wrapping(input: Result<bool, i32>, first: string, last: string, message: string, owner: string): Result<i32, i32> {
                let _ = consume(firstStage(first), (input ? error(message)), lastStage(last));
                return Result.Ok(7i32);
            }
            function option(input: Option<bool>, taken: string): Option<bool> { dispose(taken); return input; }
            function piped(input: Option<bool>, first: string, last: string, taken: string, owner: string): Option<bool> {
                let bit = consume(first, (taken |> option(input, _)?), lastStage(last));
                return Option.Some(bit);
            }
            function relay(input: Result<bool, i32>): Result<bool, i32> { return input; }
            function pipedResult(input: Result<bool, i32>, first: string, last: string): Result<i32, i32> {
                let _ = consume(first, (input |> relay?), last);
                return Result.Ok(7i32);
            }
            function bad(message: string): i32 { dispose(message); dispose(message); return 17i32; }
            function rejected(input: Result<bool, i32>, first: string, last: string, message: string): Result<i32, i32> {
                let _ = consume(first, input ? bad(message), last);
                return Result.Ok(7i32);
            }
            function moved(input: Result<bool, i32>, first: string, last: string): Result<i32, i32> {
                let _ = consume(first, input ? error(first), last);
                return Result.Ok(7i32);
            }
            function pair(first: bool, second: bool): bool { return first; }
            function pass(input: Option<bool>): Option<bool> { return input; }
            function identity(value: bool): bool { return value; }
            function unsupported(first: Option<bool>.Some, second: Option<bool>): Option<bool> {
                let bit = pair(first?, identity(second |> pass?));
                return Option.Some(bit);
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
            let result = typed.result_role().unwrap();
            let option = typed.option_role().unwrap();
            let report = check("wrapping");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("wrapping: {:?}", report.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            assert_eq!(
                calls_on_path(mir, [true]),
                [
                    &declaration("firstStage").id,
                    &declaration("lastStage").id,
                    &declaration("consume").id
                ]
            );
            assert_eq!(
                calls_on_path(mir, [false]),
                [&declaration("firstStage").id, &declaration("error").id]
            );
            let ok = linear(mir, yes);
            let err = linear(mir, no);
            assert_return(
                &ok,
                &result.ok.id,
                true,
                &[b"owner: string", b"message: string"],
            );
            let resume: Vec<_> = ok
                .iter()
                .copied()
                .filter(|op| matches!(op, Operation::TakeResultSuccess { .. } | Operation::Call(_)))
                .collect();
            assert!(
                matches!(resume.as_slice(), [
                Operation::TakeResultSuccess { operand, into: Some(slot) }, Operation::Call(last), Operation::Call(consume),
            ] if operand.root.span.bytes() == b"input" && slot.root.span.bytes() == b"(input ? error(message))"
                && *last == &declaration("lastStage").id && *consume == &declaration("consume").id),
                "{resume:?}"
            );
            assert_export(
                &err,
                &[b"firstStage(first)", b"owner: string", b"last: string"],
                |op, out| {
                    matches!(op, Operation::WrapResultFailure { operand, payload, into, variant, wrapper, origin, .. }
                    if operand.root.span.bytes() == b"input" && payload.root.span.bytes() == b"error(message)"
                    && into.root.node == out.root.node && *variant == &result.err.id && *wrapper == &result.wrapper.id && origin.bytes() == b"?"
                    && origin.snapshot() == operand.root.span.snapshot() && origin.start() == operand.root.span.end() + 1)
                },
            );
            assert!(!err.iter().any(|op| matches!(
                op,
                Operation::TakeResultSuccess { .. }
                    | Operation::ForwardResultFailure { .. }
                    | Operation::ConstructResultError { .. }
            )));
            let report = check("piped");
            let mir = report
                .mir()
                .expect("the inner pipe result is staged before its branch");
            let (yes, no) = variant_edges(
                mir,
                mir.entry,
                &option.some.id,
                b"taken |> option(input, _)?",
            );
            let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                .iter()
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
                matches!(prefix.as_slice(), [
                Operation::Move { from: first, into: Some(staged) },
                Operation::Move { from: taken, into: Some(step_arg) },
                Operation::Copy { from: input, into: Some(input_arg) },
                Operation::Call(callee), Operation::TestVariant { .. },
            ] if first.root.span.bytes() == b"first: string" && staged.root.span.bytes() == b"first"
                && taken.root.span.bytes() == b"taken: string" && step_arg.root.span.bytes() == b"taken"
                && input.root.span.bytes() == b"input: Option<bool>" && input_arg.root.span.bytes() == b"input"
                && *callee == &declaration("option").id),
                "{prefix:?}"
            );
            assert_eq!(
                calls_on_path(mir, [true]),
                [
                    &declaration("option").id,
                    &declaration("lastStage").id,
                    &declaration("consume").id
                ]
            );
            assert_eq!(calls_on_path(mir, [false]), [&declaration("option").id]);
            let ok = linear(mir, yes);
            let resume: Vec<_> = ok
                .iter()
                .copied()
                .filter(|op| matches!(op, Operation::CopyOptionPayload { .. } | Operation::Call(_)))
                .collect();
            assert!(
                matches!(resume.as_slice(), [
                Operation::CopyOptionPayload { operand, into }, Operation::Call(last), Operation::Call(consume),
            ] if operand.root.span.bytes() == b"taken |> option(input, _)?" && into.root.span.bytes() == b"(taken |> option(input, _)?)"
                && *last == &declaration("lastStage").id && *consume == &declaration("consume").id),
                "{resume:?}"
            );
            assert_return(&ok, &option.some.id, true, &[b"owner: string"]);
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"first", b"owner: string", b"last: string"],
            );
            let piped_result = check("pipedResult");
            let mir = piped_result
                .mir()
                .expect("an inner scalar Result pipe preserves the failed wrapper");
            let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, b"input |> relay?");
            assert_eq!(calls_on_path(mir, [false]), [&declaration("relay").id]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [&declaration("relay").id, &declaration("consume").id]
            );
            assert_forward(
                &linear(mir, no),
                result,
                b"input |> relay?",
                &[b"first", b"last: string"],
            );
            assert_return(&linear(mir, yes), &result.ok.id, true, &[]);
            let bad = check("rejected");
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected one reached RHS finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(message)");
            assert_eq!(finding.related.span.bytes(), b"dispose(message)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let moved = check("moved");
            assert_eq!(moved.completion(), Completion::SourceRejected);
            assert!(moved.mir().is_none());
            let [finding] = moved.findings() else {
                panic!("expected one moved argument finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"first");
            assert_eq!(finding.related.span.bytes(), b"first");
            assert!(finding.related.span.start() < finding.site.span.start());
            let unsupported = check("unsupported");
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert_eq!(
                unsupported.blocked_at().unwrap().span.bytes(),
                b"pair(first?, identity(second |> pass?))"
            );
            assert!(unsupported.mir().is_none());
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("piped", limits).mir().is_some());
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
                let limited = lower("piped", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn single_edge_arguments_do_not_lower_an_unreachable_call_path() {
    use compiler::analysis::scalars::NodeFactKind;

    probe_with_typed(
        "propagation::single_edge_arguments_do_not_lower_an_unreachable_call_path",
        r#"
            import std::result::Err as makeFailure;
            function before(value: string): string { return value; }
            function after(value: string): string { return value; }
            function consume(first: string, bit: bool, last: string): bool { dispose(last); dispose(first); return bit; }
            function unreachable(first: string, bit: bool, last: string): bool {
                dispose(first);
                dispose(first);
                dispose(last);
                return bit;
            }
            function late<T>(value: T): T { return value; }
            function success(first: string, input: Option<bool>.Some, last: string): Option<bool> {
                let bit = consume(before(first), (input?), after(last));
                return Option.Some(bit);
            }
            function failure(first: string, input: Option<bool>.None, last: string, owner: string): Option<bool> {
                let bit = unreachable(before(first), input?, late<string>(last));
                dispose(owner);
                return Option.Some(bit);
            }
            function short(first: string, bit: bool): bool { dispose(first); dispose(first); return bit; }
            function failureShort(first: string, input: Option<bool>.None, last: string, owner: string): Option<bool> {
                let bit = short(before(first), input?);
                dispose(owner);
                return Option.Some(bit);
            }
            function pair(first: bool, second: bool): bool { return first; }
            function identity(value: bool): bool { return value; }
            function deeper(input: Option<bool>.None, other: Option<bool>): Option<bool> {
                let _ = pair(input?, identity(other?));
                return Option.Some(true);
            }
            function deeperKnown(input: Option<bool>.None, other: Option<bool>.Some): Option<bool> {
                let _ = pair(input?, identity(other?));
                return Option.Some(true);
            }
            function onlyErr(input: Result<bool, i32>.Err): Result<bool, i32>.Err { return input; }
            function failurePipe(first: string, input: Result<bool, i32>.Err, last: string): Result<i32, i32> {
                let _ = unreachable(before(first), (input |> onlyErr?), late<string>(last));
                return Result.Ok(1i32);
            }
            function freshFailure(first: string, code: i32, last: string): Result<i32, i32> {
                let _ = unreachable(before(first), (code |> makeFailure<bool, i32>?), late<string>(last));
                return Result.Ok(1i32);
            }
            function firstArgument(bit: bool, last: string): bool { dispose(last); dispose(last); return bit; }
            function emptyPrefix(input: Option<bool>.None, last: string): Option<bool> {
                let _ = firstArgument(input?, late<string>(last));
                return Option.Some(true);
            }
            function withPrefix(first: string, second: string, bit: bool, last: string): bool { dispose(last); dispose(second); dispose(first); return bit; }
            function error(message: string): i32 { dispose(message); return 14i32; }
            function failureWrap(input: Result<bool, i32>.Err, first: string, second: string, last: string, message: string, owner: string): Result<i32, i32> {
                let _ = withPrefix(before(first), before(second), (input ? error(message)), late<string>(last));
                return Result.Ok(1i32);
            }
            function number(value: i32): i32 { return value; }
            function deeperWrap(input: Result<bool, i32>.Ok, first: string, last: string): Result<i32, i32> {
                let _ = consume(first, (input ? number(Result<i32, i32>.Ok(1i32)?)), last);
                return Result.Ok(1i32);
            }
            function badRhs(message: string): i32 { dispose(message); dispose(message); return 14i32; }
            function successWrap(input: Result<bool, i32>.Ok, first: string, last: string, message: string): Result<i32, i32> {
                let _ = consume(first, (input ? badRhs(message)), after(last));
                return Result.Ok(1i32);
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
            let option = typed.option_role().unwrap();
            let result = typed.result_role().unwrap();
            let dead_call = b"unreachable(before(first), input?, late<string>(last))";
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|node| node.span.bytes() == dead_call)
                .unwrap();
            let facts = typed
                .bodies()
                .node(module, node)
                .unwrap()
                .expect("typed enclosing call facts");
            assert!(!facts.flow.normal && facts.flow.returns);
            let NodeFactKind::Expression(call) = &facts.kind else {
                panic!("expected call expression facts")
            };
            assert_eq!(
                call.call.as_ref().map(|call| call.callee),
                Some(&declaration("unreachable").id)
            );
            let failed = check("failure");
            let mir = failed
                .mir()
                .unwrap_or_else(|| panic!("failure: {:?}", failed.completion()));
            assert_eq!(calls_on_path(mir, []), [&declaration("before").id]);
            let ops = linear(mir, mir.entry);
            assert_return(
                &ops,
                &option.none.id,
                false,
                &[b"before(first)", b"owner: string", b"last: string"],
            );
            assert!(!ops.iter().any(|op| matches!(
                op,
                Operation::TestVariant { .. }
                    | Operation::CopyOptionPayload { .. }
                    | Operation::Dispose { .. }
            )));
            let short = check("failureShort");
            let mir = short
                .mir()
                .expect("the shorter call has the same reached prefix");
            assert_eq!(calls_on_path(mir, []), [&declaration("before").id]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"before(first)", b"owner: string", b"last: string"],
            );
            assert_eq!(
                failed.statistics().entries,
                short.statistics().entries,
                "a dead suffix must not reserve a stage or call entry"
            );
            for (name, site) in [
                ("deeper", "pair(input?, identity(other?))"),
                ("deeperKnown", "pair(input?, identity(other?))"),
                (
                    "deeperWrap",
                    "consume(first, (input ? number(Result<i32, i32>.Ok(1i32)?)), last)",
                ),
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
            let route = check("failurePipe");
            let mir = route
                .mir()
                .expect("a known Err pipe forwards without reaching the outer call");
            assert_eq!(
                calls_on_path(mir, []),
                [&declaration("before").id, &declaration("onlyErr").id]
            );
            assert_forward(
                &linear(mir, mir.entry),
                result,
                b"input |> onlyErr?",
                &[b"before(first)", b"last: string"],
            );
            let fresh = check("freshFailure");
            let mir = fresh
                .mir()
                .expect("one fresh Err constructor can be the only feasible edge");
            assert_eq!(calls_on_path(mir, []), [&declaration("before").id]);
            let ops = linear(mir, mir.entry);
            assert_forward(
                &ops,
                result,
                b"code |> makeFailure<bool, i32>?",
                &[b"before(first)", b"last: string"],
            );
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(op, Operation::ConstructResultError { .. }))
                    .count(),
                1
            );
            let empty = check("emptyPrefix");
            let mir = empty
                .mir()
                .expect("failure in the first argument has no staged prefix");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"last: string"],
            );
            let success = check("success");
            let mir = success
                .mir()
                .expect("known Some initializes the argument and reaches the call");
            let ops = linear(mir, mir.entry);
            let stages: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Call(_)
                            | Operation::CopyOptionPayload { .. }
                            | Operation::TestVariant { .. }
                    )
                })
                .collect();
            assert!(
                matches!(stages.as_slice(), [
                Operation::Call(before), Operation::CopyOptionPayload { operand, into }, Operation::Call(after), Operation::Call(consume),
            ] if *before == &declaration("before").id && operand.root.span.bytes() == b"input" && into.root.span.bytes() == b"(input?)"
                && *after == &declaration("after").id && *consume == &declaration("consume").id),
                "{stages:?}"
            );
            assert_return(&ops, &option.some.id, true, &[]);
            let wrapping_failure = check("failureWrap");
            let mir = wrapping_failure
                .mir()
                .expect("the known failure evaluates only the prefix and RHS");
            assert_eq!(
                calls_on_path(mir, []),
                [
                    &declaration("before").id,
                    &declaration("before").id,
                    &declaration("error").id
                ]
            );
            let ops = linear(mir, mir.entry);
            assert_export(
                &ops,
                &[
                    b"before(second)",
                    b"before(first)",
                    b"owner: string",
                    b"last: string",
                ],
                |op, out| {
                    matches!(op,
                Operation::WrapResultFailure { operand, payload, into, origin, .. }
                if operand.root.span.bytes() == b"input" && payload.root.span.bytes() == b"error(message)"
                    && into.root.node == out.root.node && origin.bytes() == b"?")
                },
            );
            let wrap = check("successWrap");
            let mir = wrap
                .mir()
                .expect("known successful wrap does not lower its RHS");
            assert_eq!(
                calls_on_path(mir, []),
                [&declaration("after").id, &declaration("consume").id]
            );
            let ops = linear(mir, mir.entry);
            assert!(ops.iter().any(|op| matches!(op, Operation::TakeResultSuccess { operand, into: Some(arg) }
                if operand.root.span.bytes() == b"input" && arg.root.span.bytes() == b"(input ? badRhs(message))")));
            assert!(
                !ops.iter()
                    .any(|op| matches!(op, Operation::WrapResultFailure { .. }))
            );
            assert_return(&ops, &result.ok.id, true, &[b"message: string"]);
            let limits = AnalysisLimits {
                max_work: failed.statistics().work,
                max_entries: failed.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("failure", limits).mir().is_some());
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
                let limited = lower("failure", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn two_propagating_arguments_clean_their_own_staged_prefixes() {
    probe_with_typed(
        "propagation::two_propagating_arguments_clean_their_own_staged_prefixes",
        r#"
            import std::result::Err as fail;
            function before(value: string): string { return value; }
            function between(value: string): string { return value; }
            function after(value: string): string { return value; }
            function consume(first: string, a: bool, middle: string, b: bool, last: string): bool {
                dispose(last); dispose(middle); dispose(first); return b;
            }
            function step(input: Option<bool>, taken: string): Option<bool> { dispose(taken); return input; }
            function options(first: string, left: Option<bool>, middle: string, right: Option<bool>, taken: string, last: string, owner: string): Option<bool> {
                let bit = consume(before(first), left?, between(middle), (taken |> step(right, _)?), after(last));
                return Option.Some(bit);
            }
            function produce(flag: bool): Result<bool, i32> {
                if (flag) { return Result.Ok(true); }
                return fail<bool, i32>(17i32);
            }
            function error(message: string): i32 { dispose(message); return 19i32; }
            function results(first: string, input: Result<bool, i32>, middle: string, flag: bool, last: string, message: string, owner: string): Result<i32, i32> {
                let _ = consume(first, input?, middle, (produce(flag) ? error(message)), last);
                return Result.Ok(7i32);
            }
            function badError(message: string): i32 { dispose(message); dispose(message); return 19i32; }
            function reachedAfterFirst(first: string, input: Result<bool, i32>, middle: string, flag: bool, last: string, message: string): Result<i32, i32> {
                let _ = consume(first, input?, middle, (produce(flag) ? badError(message)), last);
                return Result.Ok(7i32);
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
            let option = typed.option_role().unwrap();
            let result = typed.result_role().unwrap();
            let report = check("options");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("options: {:?}", report.completion()));
            let (left_ok, left_none) = variant_edges(mir, mir.entry, &option.some.id, b"left");
            let (right_ok, right_none) =
                variant_edges(mir, left_ok, &option.some.id, b"taken |> step(right, _)?");
            assert_eq!(calls_on_path(mir, [false]), [&declaration("before").id]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [
                    &declaration("before").id,
                    &declaration("between").id,
                    &declaration("step").id
                ]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [
                    &declaration("before").id,
                    &declaration("between").id,
                    &declaration("step").id,
                    &declaration("after").id,
                    &declaration("consume").id
                ]
            );
            assert_return(
                &linear(mir, left_none),
                &option.none.id,
                false,
                &[
                    b"before(first)",
                    b"owner: string",
                    b"last: string",
                    b"taken: string",
                    b"middle: string",
                ],
            );
            assert_return(
                &linear(mir, right_none),
                &option.none.id,
                false,
                &[
                    b"between(middle)",
                    b"before(first)",
                    b"owner: string",
                    b"last: string",
                ],
            );
            let early: Vec<_> = straight_blocks(mir, left_ok)
                .into_iter()
                .map(|b| &b.operation)
                .filter(|op| {
                    matches!(
                        op,
                        Operation::CopyOptionPayload { .. }
                            | Operation::Call(_)
                            | Operation::TestVariant { .. }
                    )
                })
                .collect();
            assert!(
                matches!(early.as_slice(), [Operation::CopyOptionPayload { operand, into }, Operation::Call(between), Operation::Call(step), Operation::TestVariant { .. }]
                if operand.root.span.bytes() == b"left" && into.root.span.bytes() == b"left?" && *between == &declaration("between").id && *step == &declaration("step").id),
                "{early:?}"
            );
            let success = linear(mir, right_ok);
            let late: Vec<_> = success
                .iter()
                .copied()
                .filter(|op| matches!(op, Operation::CopyOptionPayload { .. } | Operation::Call(_)))
                .collect();
            assert!(
                matches!(late.as_slice(), [Operation::CopyOptionPayload { operand, into }, Operation::Call(after), Operation::Call(consume)]
                if operand.root.span.bytes() == b"taken |> step(right, _)?" && into.root.span.bytes() == b"(taken |> step(right, _)?)"
                    && *after == &declaration("after").id && *consume == &declaration("consume").id),
                "{late:?}"
            );
            assert_return(&success, &option.some.id, true, &[b"owner: string"]);
            let results = check("results");
            let mir = results
                .mir()
                .expect("the second Result can wrap only on the first Result's success path");
            let (left_ok, left_err) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            let (right_ok, right_err) =
                variant_edges(mir, left_ok, &result.ok.id, b"produce(flag)");
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [&declaration("produce").id, &declaration("error").id]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [&declaration("produce").id, &declaration("consume").id]
            );
            assert_forward(
                &linear(mir, left_err),
                result,
                b"input",
                &[
                    b"first",
                    b"owner: string",
                    b"message: string",
                    b"last: string",
                    b"middle: string",
                ],
            );
            let failure = linear(mir, right_err);
            assert_export(
                &failure,
                &[b"middle", b"first", b"owner: string", b"last: string"],
                |op, out| {
                    matches!(op,
                Operation::WrapResultFailure { operand, payload, into, origin, .. } if operand.root.span.bytes() == b"produce(flag)"
                    && payload.root.span.bytes() == b"error(message)" && into.root.node == out.root.node && origin.bytes() == b"?")
                },
            );
            assert_return(
                &linear(mir, right_ok),
                &result.ok.id,
                true,
                &[b"owner: string", b"message: string"],
            );
            let rejected = check("reachedAfterFirst");
            assert_eq!(rejected.completion(), Completion::SourceRejected);
            assert!(rejected.mir().is_none());
            let [finding] = rejected.findings() else {
                panic!("expected one reached RHS finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(message)");
            assert_eq!(finding.related.span.bytes(), b"dispose(message)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("options", limits).mir().is_some());
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
                let limited = lower("options", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn paired_arguments_use_only_their_feasible_edges() {
    probe_with_typed(
        "propagation::paired_arguments_use_only_their_feasible_edges",
        r#"
            import std::result::Err as fail;
            function before(value: string): string { return value; }
            function between(value: string): string { return value; }
            function after(value: string): string { return value; }
            function late<T>(value: T): T { return value; }
            function consume(first: string, a: bool, middle: string, b: bool, last: string): bool {
                dispose(last); dispose(middle); dispose(first); return b;
            }
            function rejected(first: string, a: bool, middle: string, b: bool, last: string): bool {
                dispose(first); dispose(first); dispose(middle); dispose(last); return b;
            }
            function firstStops(first: string, left: Option<bool>.None, middle: string, right: Option<bool>, last: string): Option<bool> {
                let _ = rejected(before(first), left?, between(middle), right?, late<string>(last));
                return Option.Some(true);
            }
            function secondStops(first: string, input: Result<bool, i32>, middle: string, code: i32, last: string): Result<i32, i32> {
                let _ = rejected(before(first), input?, between(middle), (code |> fail<bool, i32>?), late<string>(last));
                return Result.Ok(1i32);
            }
            function firstContinues(first: string, left: Option<bool>.Some, middle: string, right: Option<bool>, last: string): Option<bool> {
                let bit = consume(before(first), left?, between(middle), right?, after(last));
                return Option.Some(bit);
            }
            function secondContinues(first: string, left: Option<bool>, middle: string, right: Option<bool>.Some, last: string): Option<bool> {
                let bit = consume(before(first), left?, between(middle), right?, after(last));
                return Option.Some(bit);
            }
            function bothContinue(first: string, left: Option<bool>.Some, middle: string, right: Option<bool>.Some, last: string): Option<bool> {
                let bit = consume(before(first), left?, between(middle), right?, after(last));
                return Option.Some(bit);
            }
            function pair(a: bool, b: bool): bool { return a; }
            function nested(value: bool): Option<bool> { return Option.Some(value); }
            function deeper(left: Option<bool>.None, right: Option<bool>): Option<bool> {
                let _ = pair(left?, nested(right?)?);
                return Option.Some(true);
            }
            function number(a: bool, b: i32): bool { return a; }
            function widerSkipped(left: Option<bool>.None): Option<bool> {
                let _ = number(left?, Option<i32>.Some(1i32)?);
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
            let option = typed.option_role().unwrap();
            let result = typed.result_role().unwrap();
            let report = check("firstStops");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("firstStops: {:?}", report.completion()));
            assert_eq!(calls_on_path(mir, []), [&declaration("before").id]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"before(first)", b"last: string", b"middle: string"],
            );
            let stopped = check("secondStops");
            let mir = stopped
                .mir()
                .expect("success of the first argument reaches the failure-only second argument");
            let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            assert_eq!(calls_on_path(mir, [false]), [&declaration("before").id]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [&declaration("before").id, &declaration("between").id]
            );
            assert_forward(
                &linear(mir, no),
                result,
                b"input",
                &[b"before(first)", b"last: string", b"middle: string"],
            );
            let later = linear(mir, yes);
            assert_forward(
                &later,
                result,
                b"code |> fail<bool, i32>?",
                &[b"between(middle)", b"before(first)", b"last: string"],
            );
            assert!(later.iter().any(
                |op| matches!(op, Operation::TakeResultSuccess { operand, into: Some(into) }
                if operand.root.span.bytes() == b"input" && into.root.span.bytes() == b"input?")
            ));
            assert_eq!(
                later
                    .iter()
                    .filter(|op| matches!(op, Operation::ConstructResultError { .. }))
                    .count(),
                1
            );
            for (name, unknown, drops) in [
                (
                    "firstContinues",
                    b"right".as_slice(),
                    &[
                        b"between(middle)".as_slice(),
                        b"before(first)",
                        b"last: string",
                    ][..],
                ),
                (
                    "secondContinues",
                    b"left".as_slice(),
                    &[
                        b"before(first)".as_slice(),
                        b"last: string",
                        b"middle: string",
                    ][..],
                ),
            ] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, unknown);
                assert_eq!(
                    calls_on_path(mir, [true]),
                    [
                        &declaration("before").id,
                        &declaration("between").id,
                        &declaration("after").id,
                        &declaration("consume").id
                    ]
                );
                assert_return(&linear(mir, no), &option.none.id, false, drops);
                assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            }
            let both = check("bothContinue");
            let mir = both
                .mir()
                .expect("two success-only arguments reach the outer call with no branch");
            assert_eq!(
                calls_on_path(mir, []),
                [
                    &declaration("before").id,
                    &declaration("between").id,
                    &declaration("after").id,
                    &declaration("consume").id
                ]
            );
            let ops = linear(mir, mir.entry);
            assert_return(&ops, &option.some.id, true, &[]);
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(op, Operation::CopyOptionPayload { .. }))
                    .count(),
                2
            );
            for (name, site) in [
                ("deeper", "pair(left?, nested(right?)?)"),
                ("widerSkipped", "Option<i32>.Some(1i32)?"),
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
            let limits = AnalysisLimits {
                max_work: stopped.statistics().work,
                max_entries: stopped.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("secondStops", limits).mir().is_some());
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
                let limited = lower("secondStops", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn wider_propagating_calls_keep_each_reached_prefix() {
    probe_with_typed(
        "propagation::wider_propagating_calls_keep_each_reached_prefix",
        r#"
            function stage(value: string): string { return value; }
            function useAll(x: string, a: bool, y: string, b: bool, z: string, c: bool): bool {
                dispose(z); dispose(y); dispose(x); return c;
            }
            function never(x: string, a: bool, b: bool, y: string, c: bool, d: bool): bool {
                dispose(x); dispose(x); dispose(y); return a;
            }
            function neverNarrow(x: string, a: bool, b: bool, y: string, c: bool): bool {
                dispose(x); dispose(x); dispose(y); return a;
            }
            function three(x: string, first: Option<bool>, y: string, second: Option<bool>, z: string, third: Option<bool>): Option<bool> {
                let bit = useAll(stage(x), first?, stage(y), second?, stage(z), third?);
                return Option.Some(bit);
            }
            function stop(x: string, first: Option<bool>, second: Option<bool>.Some, y: string, third: Option<bool>.None, fourth: Option<bool>): Option<bool> {
                let _ = never(stage(x), first?, second?, stage(y), third?, fourth?);
                return Option.Some(true);
            }
            function stopNarrow(x: string, first: Option<bool>, second: Option<bool>.Some, y: string, third: Option<bool>.None, fourth: Option<bool>): Option<bool> {
                let _ = neverNarrow(stage(x), first?, second?, stage(y), third?);
                return Option.Some(true);
            }
            function reached(x: string, first: Option<bool>, second: Option<bool>, y: string, third: Option<bool>): Option<bool> {
                let bit = neverNarrow(stage(x), first?, second?, stage(y), third?);
                return Option.Some(bit);
            }
            function mixed(a: bool, b: bool, c: bool, d: bool, e: i32): bool { return a; }
            function skippedWide(first: Option<bool>, stop: Option<bool>.None, skipped: Option<bool>): Option<bool> {
                let _ = mixed(first?, stop?, skipped?, true, Option<i32>.Some(1i32)?);
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let check = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let three = check("three", default);
            let mir = three
                .mir()
                .unwrap_or_else(|| panic!("three: {:?}", three.completion()));
            let (first, no_first) = variant_edges(mir, mir.entry, &option.some.id, b"first");
            let (second, no_second) = variant_edges(mir, first, &option.some.id, b"second");
            let (third, no_third) = variant_edges(mir, second, &option.some.id, b"third");
            assert_return(
                &linear(mir, no_first),
                &option.none.id,
                false,
                &[b"stage(x)", b"z: string", b"y: string"],
            );
            assert_return(
                &linear(mir, no_second),
                &option.none.id,
                false,
                &[b"stage(y)", b"stage(x)", b"z: string"],
            );
            assert_return(
                &linear(mir, no_third),
                &option.none.id,
                false,
                &[b"stage(z)", b"stage(y)", b"stage(x)"],
            );
            assert_return(&linear(mir, third), &option.some.id, true, &[]);
            assert_eq!(
                calls_on_path(mir, [true, true, true]),
                [id("stage"), id("stage"), id("stage"), id("useAll")]
            );
            let stop = check("stop", default);
            let mir = stop.mir().expect(
                "known third failure skips the fourth argument and the invalid outer callee",
            );
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"first");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(x)", b"y: string"],
            );
            assert_return(
                &linear(mir, yes),
                &option.none.id,
                false,
                &[b"stage(y)", b"stage(x)"],
            );
            assert_eq!(calls_on_path(mir, [true]), [id("stage"), id("stage")]);
            assert!(mir.blocks.iter().all(|block| !matches!(&block.operation,
                Operation::TestVariant { operand, .. } if operand.root.span.bytes() == b"fourth")));
            let narrow = check("stopNarrow", default);
            assert!(narrow.mir().is_some());
            assert_eq!(
                stop.mir().unwrap().blocks.len(),
                narrow.mir().unwrap().blocks.len()
            );
            assert!(stop.statistics().work > narrow.statistics().work);
            let skipped = check("skippedWide", default);
            assert_eq!(skipped.completion(), Completion::Incomplete);
            assert!(skipped.mir().is_none());
            assert_eq!(
                skipped.blocked_at().unwrap().span.bytes(),
                b"Option<i32>.Some(1i32)?"
            );
            let reached = check("reached", default);
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("expected one reached-callee finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(x)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let exact = AnalysisLimits {
                max_work: three.statistics().work,
                max_entries: three.statistics().entries,
                ..default
            };
            assert!(check("three", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = check("three", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn outer_pipes_stage_the_input_before_propagating_written_arguments() {
    probe_with_typed(
        "propagation::outer_pipes_stage_the_input_before_propagating_written_arguments",
        r#"
            function input(value: string): string { return value; }
            function middle(value: string): string { return value; }
            function after(value: bool): bool { return value; }
            function consume(a: bool, mid: string, b: bool, tail: bool, owner: string): bool {
                dispose(owner); dispose(mid); return a;
            }
            function rejected(a: bool, mid: string, b: bool, tail: bool, owner: string): bool {
                dispose(owner); dispose(owner); dispose(mid); return a;
            }
            function pipe(owner: string, first: Option<bool>, mid: string, second: Option<bool>): Option<bool> {
                let bit = input(owner) |> consume(first?, middle(mid), second?, after(true), _);
                return Option.Some(bit);
            }
            function stop(owner: string, first: Option<bool>, mid: string, second: Option<bool>.None): Option<bool> {
                let _ = input(owner) |> rejected(first?, middle(mid), second?, after(true), _);
                return Option.Some(false);
            }
            function implicit(owner: string, first: Option<bool>): Option<bool> {
                let bit = input(owner) |> head(first?);
                return Option.Some(bit);
            }
            function moved(owner: string, first: Option<bool>, second: Option<bool>): Option<bool> {
                let bit = input(owner) |> consume(first?, middle(owner), second?, after(true), _);
                return Option.Some(bit);
            }
            function head(owner: string, bit: bool): bool { dispose(owner); return bit; }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let check = |name| lower(name, default);
            let option = typed.option_role().unwrap();
            let report = check("pipe");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("pipe: {:?}", report.completion()));
            let (first, no_first) = variant_edges(mir, mir.entry, &option.some.id, b"first");
            let (second, no_second) = variant_edges(mir, first, &option.some.id, b"second");
            assert_return(
                &linear(mir, no_first),
                &option.none.id,
                false,
                &[b"input(owner)", b"mid: string"],
            );
            assert_return(
                &linear(mir, no_second),
                &option.none.id,
                false,
                &[b"middle(mid)", b"input(owner)"],
            );
            assert_return(&linear(mir, second), &option.some.id, true, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("input")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("input"), id("middle")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("input"), id("middle"), id("after"), id("consume")]
            );
            let stopped = check("stop");
            let mir = stopped
                .mir()
                .expect("known second failure still evaluates a later placeholder first");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"first");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"input(owner)", b"mid: string"],
            );
            assert_return(
                &linear(mir, yes),
                &option.none.id,
                false,
                &[b"middle(mid)", b"input(owner)"],
            );
            assert_eq!(calls_on_path(mir, [true]), [id("input"), id("middle")]);
            let implicit = check("implicit");
            let mir = implicit
                .mir()
                .expect("implicit first parameter with a written propagation");
            let (_, no) = variant_edges(mir, mir.entry, &option.some.id, b"first");
            assert_return(&linear(mir, no), &option.none.id, false, &[b"input(owner)"]);
            assert_eq!(calls_on_path(mir, [true]), [id("input"), id("head")]);
            let moved = check("moved");
            assert_eq!(moved.completion(), Completion::SourceRejected);
            assert!(moved.mir().is_none());
            let [finding] = moved.findings() else {
                panic!("expected one moved-input finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"owner");
            assert_eq!(finding.related.span.bytes(), b"owner");
            assert!(finding.related.span.start() < finding.site.span.start());
            let exact = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..default
            };
            assert!(lower("pipe", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("pipe", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn propagating_pipe_input_runs_before_written_arguments() {
    probe_with_typed(
        "propagation::propagating_pipe_input_runs_before_written_arguments",
        r#"
            function relay(value: Option<bool>): Option<bool> { return value; }
            function stage(owner: string): string { return owner; }
            function consume(owner: string, written: bool, incoming: bool): bool {
                dispose(owner); return incoming;
            }
            function invalid(owner: string, written: bool, incoming: bool): bool {
                dispose(owner); dispose(owner); return incoming;
            }
            function pipe(input: Option<bool>, owner: string, second: Option<bool>): Option<bool> {
                let bit = (relay(input)?) |> consume(stage(owner), second?, _);
                return Option.Some(bit);
            }
            function stop(input: Option<bool>.None, owner: string, second: Option<bool>): Option<bool> {
                let _ = (input?) |> invalid(stage(owner), second?, _);
                return Option.Some(true);
            }
            function bad(input: Option<bool>, owner: string, second: Option<bool>): Option<bool> {
                let bit = (input?) |> invalid(stage(owner), second?, _);
                return Option.Some(bit);
            }
            function identity(bit: bool): bool { return bit; }
            function present(input: Option<bool>.Some): Option<bool> {
                let bit = (input?) |> identity;
                return Option.Some(bit);
            }
            function errorInput(input: Result<bool, i32>, owner: string): Result<bool, i32> {
                let bit = (input?) |> consume(stage(owner), true, _);
                return Result.Ok(bit);
            }
            function numbers(value: i32, bit: bool): bool { return bit; }
            function unsupportedSkipped(input: Option<bool>.None): Option<bool> {
                let bit = (input?) |> numbers(Option<i32>.Some(1i32)?, _);
                return Option.Some(bit);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let report = lower("pipe", default);
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("pipe input: {:?}", report.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"relay(input)");
            let (both, second_no) = variant_edges(mir, yes, &option.some.id, b"second");
            let staged: Vec<_> = straight_blocks(mir, yes)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| matches!(op, Operation::CopyOptionPayload { .. } | Operation::Call(_)))
                .collect();
            assert!(
                matches!(staged.as_slice(), [Operation::CopyOptionPayload { operand, into }, Operation::Call(callee)]
                if operand.root.span.bytes() == b"relay(input)" && into.root.span.bytes() == b"(relay(input)?)" && *callee == id("stage")),
                "{staged:?}"
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"owner: string"],
            );
            assert_return(
                &linear(mir, second_no),
                &option.none.id,
                false,
                &[b"stage(owner)"],
            );
            assert_return(&linear(mir, both), &option.some.id, true, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("relay")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("relay"), id("stage")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("relay"), id("stage"), id("consume")]
            );
            let stopped = lower("stop", default);
            let mir = stopped
                .mir()
                .expect("known-empty input skips all written arguments and the outer call");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"owner: string"],
            );
            let bad = lower("bad", default);
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected one reached-callee finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(owner)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let present = lower("present", default);
            let mir = present
                .mir()
                .expect("known-present input has no test or failure edge");
            assert_eq!(calls_on_path(mir, []), [id("identity")]);
            assert_return(&linear(mir, mir.entry), &option.some.id, true, &[]);
            let error = lower("errorInput", default);
            let mir = error
                .mir()
                .expect("scalar Result input retains its original Err wrapper");
            let result = typed.result_role().unwrap();
            let (ok, err) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            assert_forward(&linear(mir, err), result, b"input", &[b"owner: string"]);
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(calls_on_path(mir, [true]), [id("stage"), id("consume")]);
            assert_return(&linear(mir, ok), &result.ok.id, true, &[]);
            let unsupported = lower("unsupportedSkipped", default);
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert!(unsupported.mir().is_none());
            assert_eq!(
                unsupported.blocked_at().unwrap().span.bytes(),
                b"Option<i32>.Some(1i32)?"
            );
            let exact = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..default
            };
            assert!(lower("pipe", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("pipe", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn propagating_pipe_step_runs_only_after_all_arguments_continue() {
    probe_with_typed(
        "propagation::propagating_pipe_step_runs_only_after_all_arguments_continue",
        r#"
            function relay(value: Option<bool>): Option<bool> { return value; }
            function stage(owner: string): string { return owner; }
            function finish(owner: string, written: bool, input: bool): Option<bool> {
                dispose(owner); return Option.Some(input);
            }
            function invalid(owner: string, written: bool, input: bool): Option<bool> {
                dispose(owner); dispose(owner); return Option.Some(input);
            }
            function empty(owner: string, bit: bool): Option<bool>.None { dispose(owner); return Option.None; }
            function pipe(first: Option<bool>, owner: string, second: Option<bool>): Option<bool> {
                let bit = (relay(first)?) |> finish(stage(owner), second?, _)?;
                return Option.Some(bit);
            }
            function stop(first: Option<bool>.None, owner: string, second: Option<bool>): Option<bool> {
                let _ = (first?) |> invalid(stage(owner), second?, _)?;
                return Option.Some(true);
            }
            function writtenOnly(owner: string, input: Option<bool>): Option<bool> {
                let _ = stage(owner) |> empty(input?)?;
                return Option.Some(true);
            }
            function present(owner: string, extra: string, bit: bool): Option<bool>.Some {
                dispose(extra); dispose(owner); return Option.Some(bit);
            }
            function stopWritten(owner: string, extra: string, input: Option<bool>.None): Option<bool> {
                let _ = stage(owner) |> present(stage(extra), input?)?;
                return Option.Some(true);
            }
            function stepPresent(owner: string, extra: string, input: Option<bool>): Option<bool> {
                let bit = stage(owner) |> present(stage(extra), input?)?;
                return Option.Some(bit);
            }
            function plain(owner: string, extra: string, bit: bool): bool {
                dispose(extra); dispose(owner); return bit;
            }
            function stopPlain(owner: string, extra: string, input: Option<bool>.None): Option<bool> {
                let _ = stage(owner) |> plain(stage(extra), input?);
                return Option.Some(true);
            }
            function bad(first: Option<bool>, owner: string, second: Option<bool>): Option<bool> {
                let _ = (first?) |> invalid(stage(owner), second?, _)?;
                return Option.Some(true);
            }
            function finishResult(owner: string, bit: bool): Result<bool, i32> { dispose(owner); return Result.Ok(bit); }
            function resultPipe(input: Result<bool, i32>, owner: string): Result<bool, i32> {
                let bit = (input?) |> finishResult(stage(owner), _)?;
                return Result.Ok(bit);
            }
            function nested(bit: bool): bool { return bit; }
            function deeper(owner: string, input: Option<bool>, other: Option<bool>): Option<bool> {
                let _ = stage(owner) |> finish(input?, nested(other?))?;
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| {
                lower_body(receipt, module, id(name), limits)
                    .unwrap_or_else(|error| panic!("{name}: {error:?}"))
            };
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let report = lower("pipe", default);
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("pipe step: {:?}", report.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"relay(first)");
            let (both, second_no) = variant_edges(mir, yes, &option.some.id, b"second");
            let (step_yes, step_no) = variant_edges(
                mir,
                both,
                &option.some.id,
                b"(relay(first)?) |> finish(stage(owner), second?, _)?",
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"owner: string"],
            );
            assert_return(
                &linear(mir, second_no),
                &option.none.id,
                false,
                &[b"stage(owner)"],
            );
            assert_return(&linear(mir, step_no), &option.none.id, false, &[]);
            assert_return(&linear(mir, step_yes), &option.some.id, true, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("relay")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("relay"), id("stage")]
            );
            for branch in [true, false] {
                assert_eq!(
                    calls_on_path(mir, [true, true, branch]),
                    [id("relay"), id("stage"), id("finish")]
                );
            }
            let stopped = lower("stop", default);
            let mir = stopped
                .mir()
                .expect("known input failure bypasses even the step's carrier branch");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"owner: string"],
            );
            assert!(
                !mir.blocks
                    .iter()
                    .any(|block| matches!(block.operation, Operation::TestVariant { .. }))
            );
            let written = lower("writtenOnly", default);
            let mir = written
                .mir()
                .expect("known step failure happens after the written argument succeeds");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(&linear(mir, no), &option.none.id, false, &[b"stage(owner)"]);
            assert_return(&linear(mir, yes), &option.none.id, false, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(calls_on_path(mir, [true]), [id("stage"), id("empty")]);
            let stop_written = lower("stopWritten", default);
            let mir = stop_written
                .mir()
                .expect("known written failure drops both stages without calling the step");
            assert_eq!(calls_on_path(mir, []), [id("stage"), id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(extra)", b"stage(owner)"],
            );
            let plain = lower("stopPlain", default);
            assert!(plain.mir().is_some());
            assert_eq!(
                stop_written.statistics().entries,
                plain.statistics().entries
            );
            let step_present = lower("stepPresent", default);
            let mir = step_present
                .mir()
                .expect("known successful step needs no additional variant test");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(extra)", b"stage(owner)"],
            );
            assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("stage"), id("present")]
            );
            let bad = lower("bad", default);
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected one reached-callee finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(owner)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let result_pipe = lower("resultPipe", default);
            let mir = result_pipe
                .mir()
                .expect("both propagation sites forward their own wrapper");
            let result = typed.result_role().unwrap();
            let (ok, err) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            let step = b"(input?) |> finishResult(stage(owner), _)?";
            let (step_ok, step_err) = variant_edges(mir, ok, &result.ok.id, step);
            assert_forward(&linear(mir, err), result, b"input", &[b"owner: string"]);
            assert_forward(&linear(mir, step_err), result, step, &[]);
            assert_return(&linear(mir, step_ok), &result.ok.id, true, &[]);
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("stage"), id("finishResult")]
            );
            let deeper = lower("deeper", default);
            assert_eq!(deeper.completion(), Completion::Incomplete);
            assert!(deeper.mir().is_none());
            let exact = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..default
            };
            assert!(lower("pipe", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("pipe", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn composed_pipe_statements_discard_only_the_success_payload() {
    probe_with_typed(
        "propagation::composed_pipe_statements_discard_only_the_success_payload",
        r#"
            function stage(owner: string): string { return owner; }
            function continueWith(owner: string): bool { dispose(owner); return true; }
            function optionStep(owner: string, bit: bool): Option<bool> { dispose(owner); return Option.Some(bit); }
            function resultStep(owner: string, bit: bool): Result<i32, i32> { dispose(owner); return Result.Ok(1i32); }
            function options(owner: string, input: Option<bool>, later: string): Option<bool> {
                (stage(owner) |> optionStep(input?)?);
                let bit = continueWith(later);
                return Option.Some(bit);
            }
            function results(input: Result<bool, i32>, owner: string, later: string): Result<bool, i32> {
                ((input?) |> resultStep(stage(owner), _)?);
                let bit = continueWith(later);
                return Result.Ok(bit);
            }
            function knownEmpty(input: Option<bool>.None, owner: string, later: string): Option<bool> {
                (input?) |> optionStep(stage(owner), _)?;
                let bit = continueWith(later);
                return Option.Some(bit);
            }
            function nested(bit: bool): bool { return bit; }
            function deeper(owner: string, input: Option<bool>, other: Option<bool>): Option<bool> {
                stage(owner) |> optionStep(nested(input? || other?))?;
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let options = lower("options", default);
            let mir = options
                .mir()
                .unwrap_or_else(|| panic!("options: {:?}", options.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (step_yes, step_no) = variant_edges(
                mir,
                yes,
                &option.some.id,
                b"stage(owner) |> optionStep(input?)?",
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(owner)", b"later: string"],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            let end = linear(mir, step_yes);
            assert_return(&end, &option.some.id, true, &[]);
            assert!(
                end.iter()
                    .all(|op| !matches!(op, Operation::CopyOptionPayload { .. }))
            );
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("stage"), id("optionStep")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("stage"), id("optionStep"), id("continueWith")]
            );
            let result = typed.result_role().unwrap();
            let results = lower("results", default);
            let mir = results
                .mir()
                .expect("the result step's success payload is discarded, not its Err");
            let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            let source = b"(input?) |> resultStep(stage(owner), _)?";
            let (step_yes, step_no) = variant_edges(mir, yes, &result.ok.id, source);
            assert_forward(
                &linear(mir, no),
                result,
                b"input",
                &[b"later: string", b"owner: string"],
            );
            assert_forward(&linear(mir, step_no), result, source, &[b"later: string"]);
            let end = linear(mir, step_yes);
            assert!(end.iter().any(
                |op| matches!(op, Operation::TakeResultSuccess { operand, into: None }
                if operand.root.span.bytes() == source)
            ));
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("stage"), id("resultStep"), id("continueWith")]
            );
            assert_return(&end, &result.ok.id, true, &[]);
            let known_empty = lower("knownEmpty", default);
            let mir = known_empty
                .mir()
                .expect("known empty input skips the outer step and the next statement");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"later: string", b"owner: string"],
            );
            let blocked = lower("deeper", default);
            assert_eq!(blocked.completion(), Completion::Incomplete);
            assert!(blocked.mir().is_none());
            let exact = AnalysisLimits {
                max_work: results.statistics().work,
                max_entries: results.statistics().entries,
                ..default
            };
            assert!(lower("results", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("results", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn scalar_call_statements_preserve_propagating_argument_exits() {
    probe_with_typed(
        "propagation::scalar_call_statements_preserve_propagating_argument_exits",
        r#"
            function stage(owner: string): string { return owner; }
            function after(owner: string): bool { dispose(owner); return true; }
            function consume(bit: bool, owner: string): i32 { dispose(owner); return 1i32; }
            function invalid(bit: bool, owner: string): i32 { dispose(owner); dispose(owner); return 1i32; }
            function direct(owner: string, input: Option<bool>, later: string): Option<bool> {
                (consume(input?, stage(owner)));
                let bit = after(later);
                return Option.Some(bit);
            }
            function pipe(owner: string, input: Result<bool, i32>, later: string): Result<bool, i32> {
                stage(owner) |> consume(input?, _);
                let bit = after(later);
                return Result.Ok(bit);
            }
            function stop(owner: string, input: Option<bool>.None, later: string): Option<bool> {
                stage(owner) |> invalid(input?, _);
                let bit = after(later);
                return Option.Some(bit);
            }
            function bad(owner: string, input: Option<bool>): Option<bool> {
                invalid(input?, stage(owner));
                return Option.Some(true);
            }

            function identity(bit: bool): bool { return bit; }
            function inputPipe(input: Option<bool>): Option<bool> {
                (input?) |> identity;
                return Option.Some(true);
            }
            function consumePair(owner: string, written: bool, input: bool): bool { dispose(owner); return input; }
            function pairedPipe(first: Option<bool>, owner: string, second: Option<bool>, later: string): Option<bool> {
                (first?) |> consumePair(stage(owner), second?, _);
                let bit = after(later);
                return Option.Some(bit);
            }
            function deeper(input: Option<bool>): Option<bool> {
                identity(identity(input?));
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let direct = lower("direct", default);
            let mir = direct
                .mir()
                .unwrap_or_else(|| panic!("direct: {:?}", direct.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"later: string", b"owner: string"],
            );
            assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("consume"), id("after")]
            );
            let pipe = lower("pipe", default);
            let mir = pipe
                .mir()
                .expect("pipe input stages before failure of a written argument");
            let staging: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| matches!(op, Operation::Call(_) | Operation::TestVariant { .. }))
                .collect();
            assert!(
                matches!(staging.as_slice(), [Operation::Call(callee), Operation::TestVariant { operand, .. }]
                if *callee == id("stage") && operand.root.span.bytes() == b"input"),
                "{staging:?}"
            );
            let result = typed.result_role().unwrap();
            let (yes, no) = variant_edges(mir, mir.entry, &result.ok.id, b"input");
            assert_forward(
                &linear(mir, no),
                result,
                b"input",
                &[b"stage(owner)", b"later: string"],
            );
            assert_return(&linear(mir, yes), &result.ok.id, true, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("consume"), id("after")]
            );
            let stop = lower("stop", default);
            let mir = stop
                .mir()
                .expect("known failure skips the invalid callee and the next statement");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(owner)", b"later: string"],
            );
            let bad = lower("bad", default);
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected one reached-callee finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(owner)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let input_pipe = lower("inputPipe", default);
            let mir = input_pipe
                .mir()
                .expect("ordinary pipe with a propagating input and a bool result");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(calls_on_path(mir, [true]), [id("identity")]);
            assert_return(&linear(mir, no), &option.none.id, false, &[]);
            assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            let paired = lower("pairedPipe", default);
            let mir = paired
                .mir()
                .expect("both propagating arguments share the statement continuation");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"first");
            let (both, second_no) = variant_edges(mir, yes, &option.some.id, b"second");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"later: string", b"owner: string"],
            );
            assert_return(
                &linear(mir, second_no),
                &option.none.id,
                false,
                &[b"stage(owner)", b"later: string"],
            );
            assert_return(&linear(mir, both), &option.some.id, true, &[]);
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(calls_on_path(mir, [true, false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("stage"), id("consumePair"), id("after")]
            );
            let deeper = lower("deeper", default);
            assert_eq!(deeper.completion(), Completion::Incomplete);
            assert!(deeper.mir().is_none());
            let exact = AnalysisLimits {
                max_work: pipe.statistics().work,
                max_entries: pipe.statistics().entries,
                ..default
            };
            assert!(lower("pipe", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("pipe", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn propagating_calls_create_owning_results_only_on_success() {
    probe_with_typed(
        "propagation::propagating_calls_create_owning_results_only_on_success",
        r#"
            function stage(owner: string): string { return owner; }
            function text(bit: bool, owner: string): string { return owner; }
            function scalar(bit: bool, owner: string): bool { dispose(owner); return bit; }
            function after(bit: bool): bool { return bit; }
            function named(owner: string, input: Option<bool>): Option<bool> {
                let output = text(input?, stage(owner));
                let bit = after(true);
                return Option.Some(bit);
            }
            function discarded(owner: string, input: Option<bool>): Option<bool> {
                let _ = text(input?, stage(owner));
                let bit = after(true);
                return Option.Some(bit);
            }
            function pipe(owner: string, input: Result<bool, i32>): Result<bool, i32> {
                stage(owner) |> text(input?, _);
                let bit = after(true);
                return Result.Ok(bit);
            }
            function stopped(owner: string, input: Option<bool>.None): Option<bool> {
                stage(owner) |> text(input?, _);
                let bit = after(true);
                return Option.Some(bit);
            }
            function stoppedScalar(owner: string, input: Option<bool>.None): Option<bool> {
                stage(owner) |> scalar(input?, _);
                let bit = after(true);
                return Option.Some(bit);
            }
            function stoppedNamed(owner: string, input: Option<bool>.None): Option<bool> {
                let output = stage(owner) |> text(input?, _);
                dispose(output);
                return Option.Some(true);
            }
            function reused(owner: string, input: Option<bool>): Option<bool> {
                let output = text(input?, stage(owner));
                dispose(output); dispose(output);
                return Option.Some(true);
            }
            function compound(bit: bool): [string, string] { return ["left", "right"]; }
            function unsupported(input: Option<bool>): Option<bool> {
                compound(input?);
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let default = AnalysisLimits::default();
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let option = typed.option_role().unwrap();
            let named = lower("named", default);
            let mir = named
                .mir()
                .unwrap_or_else(|| panic!("named: {:?}", named.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"owner: string"],
            );
            assert_return(&linear(mir, yes), &option.some.id, true, &[b"output"]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("text"), id("after")]
            );
            let discarded = lower("discarded", default);
            let mir = discarded
                .mir()
                .expect("explicit discard must drop the returned owner on success");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"owner: string"],
            );
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("text"), id("after")]
            );
            let result = typed.result_role().unwrap();
            let pipe = lower("pipe", default);
            let piped = pipe
                .mir()
                .expect("ordinary pipe drops its owning result at the statement boundary");
            let (pipe_yes, pipe_no) = variant_edges(piped, piped.entry, &result.ok.id, b"input");
            assert_forward(
                &linear(piped, pipe_no),
                result,
                b"input",
                &[b"stage(owner)"],
            );
            assert_eq!(calls_on_path(piped, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(piped, [true]),
                [id("stage"), id("text"), id("after")]
            );
            for (mir, entry, output) in [
                (mir, yes, b"text(input?, stage(owner))".as_slice()),
                (
                    piped,
                    pipe_yes,
                    b"stage(owner) |> text(input?, _)".as_slice(),
                ),
            ] {
                let tail = linear(mir, entry);
                let relevant: Vec<_> = tail
                    .iter()
                    .copied()
                    .filter(|op| match op {
                        Operation::Cleanup(actions) => actions
                            .iter()
                            .any(|action| matches!(action, Obligation::Drop(_))),
                        Operation::Call(callee) => *callee != id("stage"),
                        _ => false,
                    })
                    .collect();
                assert!(
                    matches!(relevant.as_slice(), [Operation::Call(text), Operation::Cleanup(actions), Operation::Call(after)]
                    if *text == id("text") && *after == id("after") && matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.span.bytes() == output)),
                    "{relevant:?}"
                );
            }
            let stopped = lower("stopped", default);
            let mir = stopped
                .mir()
                .expect("known failure never allocates the discarded result");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(owner)"],
            );
            let scalar = lower("stoppedScalar", default);
            assert!(scalar.mir().is_some());
            assert_eq!(stopped.statistics().entries, scalar.statistics().entries);
            let stopped_named = lower("stoppedNamed", default);
            let mir = stopped_named
                .mir()
                .expect("an unreachable named output stays uninitialized");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(owner)"],
            );
            let reused = lower("reused", default);
            assert_eq!(reused.completion(), Completion::SourceRejected);
            assert!(reused.mir().is_none());
            let [finding] = reused.findings() else {
                panic!("expected one moved-result finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(output)");
            assert_eq!(finding.related.span.bytes(), b"dispose(output)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let unsupported = lower("unsupported", default);
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert!(unsupported.mir().is_none());
            assert_eq!(
                unsupported.blocked_at().unwrap().span.bytes(),
                b"compound(input?)"
            );
            let exact = AnalysisLimits {
                max_work: pipe.statistics().work,
                max_entries: pipe.statistics().entries,
                ..default
            };
            assert!(lower("pipe", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("pipe", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn option_text_propagation_moves_or_drops_the_owned_payload() {
    probe_with_typed(
        "propagation::option_text_propagation_moves_or_drops_the_owned_payload",
        r#"
            function after(bit: bool): bool { return bit; }
            function named(input: Option<string>, other: string): Option<bool> {
                let held = input;
                let output = (held?);
                let bit = after(true);
                return Option.Some(bit);
            }
            function discarded(input: Option<string>, other: string): Option<bool> {
                let _ = input?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function present(input: Option<string>.Some): Option<bool> {
                (input?);
                let bit = after(true);
                return Option.Some(bit);
            }
            function absent(input: Option<string>.None, other: string): Option<bool> {
                let _ = input?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function reused(input: Option<string>.Some): Option<bool> {
                let item = input?;
                let _ = input?;
                return Option.Some(true);
            }
            function relay(value: Option<string>): Option<string> { return value; }
            function piped(input: Option<string>): Option<bool> {
                let _ = input |> relay?;
                return Option.Some(true);
            }
            function copyNone(input: Option<string>.None): Option<string>.None {
                let held = input;
                return input;
            }
            function borrowCarrier(input: Option<string>): Option<bool> {
                let _ = &input;
                let _ = input?;
                return Option.Some(true);
            }
            function laterExit(input: Option<string>.Some, flag: Option<bool>): Option<bool> {
                let output = input?;
                let bit = flag?;
                dispose(output);
                return Option.Some(bit);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let named = lower("named", default);
            let mir = named.mir().unwrap_or_else(|| {
                panic!(
                    "named: {:?} at {:?}",
                    named.completion(),
                    named
                        .blocked_at()
                        .map(|site| site.span.bytes().escape_ascii().to_string())
                )
            });
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"held");
            assert!(linear(mir, yes).iter().any(
                |op| matches!(op, Operation::TakeOptionPayload { operand, into }
                if operand.root.span.bytes() == b"held" && into.root.span.bytes() == b"output")
            ));
            let failure = linear(mir, no);
            assert!(failure.iter().any(|op| matches!(op, Operation::EndEmptyOption(operand) if operand.root.span.bytes() == b"held")));
            assert_return(&failure, &option.none.id, false, &[b"other: string"]);
            assert_return(
                &linear(mir, yes),
                &option.some.id,
                true,
                &[b"output", b"other: string"],
            );
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(calls_on_path(mir, [true]), [id("after")]);
            let discarded = lower("discarded", default);
            let mir = discarded
                .mir()
                .expect("owned success payload drops before the next call");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"other: string"],
            );
            let success = linear(mir, yes);
            assert!(
                matches!(path_drops(mir, yes).as_slice(), [Obligation::Drop(payload), Obligation::Drop(other)]
                if payload.root.span.bytes() == b"input?" && other.root.span.bytes() == b"other: string")
            );
            let [drop] = success.iter().enumerate().filter_map(|(index, op)| matches!(op, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.span.bytes() == b"input?")).then_some(index)).collect::<Vec<_>>()[..]
            else { panic!("the discarded owning payload must drop exactly once") };
            let take = success
                .iter()
                .position(|op| {
                    matches!(op, Operation::TakeOptionPayload { operand, into }
                if operand.root.span.bytes() == b"input" && into.root.span.bytes() == b"input?")
                })
                .unwrap();
            assert_return(
                &success[drop + 1..],
                &option.some.id,
                true,
                &[b"other: string"],
            );
            let call = success
                .iter()
                .position(|op| matches!(op, Operation::Call(callee) if *callee == id("after")))
                .unwrap();
            assert!(take < drop && drop < call);
            let present = lower("present", default);
            let mir = present
                .mir()
                .expect("known Some with an owning payload has no variant test");
            assert_eq!(calls_on_path(mir, []), [id("after")]);
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(payload)] if payload.root.span.bytes() == b"input?")
            );
            let present_ops = linear(mir, mir.entry);
            let relevant: Vec<_> = present_ops
                .iter()
                .copied()
                .filter(|op| match op {
                    Operation::TakeOptionPayload { .. } | Operation::Call(_) => true,
                    Operation::Cleanup(actions) => actions
                        .iter()
                        .any(|action| matches!(action, Obligation::Drop(_))),
                    _ => false,
                })
                .collect();
            assert!(
                matches!(relevant.as_slice(), [Operation::TakeOptionPayload { into, .. }, Operation::Cleanup(actions), Operation::Call(callee)]
                if *callee == id("after") && matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.node == into.root.node)),
                "{relevant:?}"
            );
            let absent = lower("absent", default);
            let mir = absent.mir().expect("known None does not create a payload");
            assert!(calls_on_path(mir, []).is_empty());
            let none = linear(mir, mir.entry);
            assert!(none.iter().any(|op| matches!(op, Operation::Move { from, into: Some(into) }
                if from.root.span.bytes() == b"input: Option<string>.None" && into.root.span.bytes() == b"input")));
            assert!(none.iter().any(|op| matches!(op, Operation::EndEmptyOption(operand) if operand.root.span.bytes() == b"input")));
            assert!(none.iter().all(|op| !matches!(
                op,
                Operation::TakeOptionPayload { .. } | Operation::TestVariant { .. }
            )));
            assert_return(&none, &option.none.id, false, &[b"other: string"]);
            let copied = lower("copyNone", default);
            let mir = copied
                .mir()
                .unwrap_or_else(|| panic!("copyNone: {:?}", copied.completion()));
            let ops = linear(mir, mir.entry);
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(op, Operation::Copy { from, .. }
                if from.root.span.bytes() == b"input: Option<string>.None"))
                    .count(),
                2
            );
            assert!(path_drops(mir, mir.entry).is_empty());
            let borrowed = lower("borrowCarrier", default);
            assert_eq!(borrowed.completion(), Completion::Incomplete);
            assert!(borrowed.mir().is_none());
            assert_eq!(borrowed.blocked_at().unwrap().span.bytes(), b"&input");
            let later = lower("laterExit", default);
            let mir = later
                .mir()
                .expect("a later early exit drops the initialized string once");
            let (_, no) = variant_edges(mir, mir.entry, &option.some.id, b"flag");
            assert_return(&linear(mir, no), &option.none.id, false, &[b"output"]);
            let reused = lower("reused", default);
            assert_eq!(reused.completion(), Completion::SourceRejected);
            assert!(reused.mir().is_none());
            let [finding] = reused.findings() else {
                panic!("expected one moved-carrier finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"input");
            assert_eq!(finding.related.span.bytes(), b"input");
            assert!(finding.related.span.start() < finding.site.span.start());
            let piped = lower("piped", default);
            assert_eq!(piped.completion(), Completion::Incomplete);
            assert!(piped.mir().is_none());
            assert_eq!(piped.blocked_at().unwrap().span.bytes(), b"input |> relay?");
            let exact = AnalysisLimits {
                max_work: discarded.statistics().work,
                max_entries: discarded.statistics().entries,
                ..default
            };
            assert!(lower("discarded", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("discarded", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn live_option_text_cleanup_drops_only_the_initialized_carrier_payload() {
    probe_with_typed(
        "propagation::live_option_text_cleanup_drops_only_the_initialized_carrier_payload",
        r#"
            function normal(before: string, carrier: Option<string>, after: string): bool { return true; }
            function early(before: string, carrier: Option<string>, input: Option<bool>, after: string): Option<bool> {
                let bit = input?;
                return Option.Some(bit);
            }
            function conditional(flag: bool, before: string, carrier: Option<string>, after: string): Option<bool> {
                if (flag) { let _ = carrier?; }
                return Option.Some(true);
            }
            function some(carrier: Option<string>.Some): bool { return true; }
            function none(carrier: Option<string>.None): bool { return true; }
            function exported(carrier: Option<string>): Option<string> {
                let held = carrier;
                return held;
            }
            function replace(old: Option<string>, incoming: Option<string>): Option<string> {
                let mut held = old;
                held = incoming;
                return held;
            }
            function selfReplace(carrier: Option<string>): Option<string> {
                let mut held = carrier;
                held = held;
                return held;
            }
            function afterDrop(bit: bool): bool { return bit; }
            function discarded(carrier: Option<string>): bool {
                let _ = carrier;
                return afterDrop(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let normal = lower("normal", default);
            let mir = normal
                .mir()
                .unwrap_or_else(|| panic!("normal: {:?}", normal.completion()));
            let option = typed.option_role().unwrap();
            let assert_carrier_cleanup = |actions: &[Obligation<'_>]| {
                assert!(
                    matches!(actions, [Obligation::Drop(after), Obligation::DropOptionPayload { operand, some }, Obligation::Drop(before)]
                    if after.root.span.bytes() == b"after: string" && operand.root.span.bytes() == b"carrier: Option<string>"
                        && *some == &option.some.id && before.root.span.bytes() == b"before: string"),
                    "{actions:?}"
                );
            };
            let assert_export_first = |mir: &Mir<'_>, entry| {
                let ops = linear(mir, entry);
                let export = ops
                    .iter()
                    .position(|op| matches!(op, Operation::Export(_)))
                    .unwrap();
                assert!(ops[..export].iter().all(|op| !matches!(
                    op,
                    Operation::Cleanup(_) | Operation::Drop(_) | Operation::DropCapture { .. }
                )));
                assert_carrier_cleanup(&path_drops(mir, entry));
            };
            assert_export_first(mir, mir.entry);
            let early = lower("early", default);
            let mir = early
                .mir()
                .expect("live carrier survives until either lexical exit");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            for entry in [yes, no] {
                assert_export_first(mir, entry);
            }
            let conditional = lower("conditional", default);
            let mir = conditional
                .mir()
                .expect("the join cleans only the carrier still initialized there");
            let route = straight_blocks(mir, mir.entry);
            let Terminator::Branch { yes, no } = route.last().unwrap().terminator else {
                panic!("expected the flag split")
            };
            assert_carrier_cleanup(&path_drops(mir, no));
            let (yes, no) = variant_edges(mir, yes, &option.some.id, b"carrier");
            assert!(
                matches!(path_drops(mir, yes).as_slice(), [Obligation::Drop(payload), Obligation::Drop(after), Obligation::Drop(before)]
                if payload.root.span.bytes() == b"carrier?" && after.root.span.bytes() == b"after: string" && before.root.span.bytes() == b"before: string")
            );
            assert!(
                matches!(path_drops(mir, no).as_slice(), [Obligation::Drop(after), Obligation::Drop(before)]
                if after.root.span.bytes() == b"after: string" && before.root.span.bytes() == b"before: string")
            );
            let some = lower("some", default);
            let mir = some
                .mir()
                .expect("statically known Some requires payload cleanup");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::DropOptionPayload { operand, some }]
                if operand.root.span.bytes() == b"carrier: Option<string>.Some" && *some == &option.some.id)
            );
            let none = lower("none", default);
            let mir = none.mir().expect("known None only ends carrier storage");
            assert!(path_drops(mir, mir.entry).is_empty());
            assert!(linear(mir, mir.entry).iter().any(|op| matches!(op, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::EndStorage(carrier)] if carrier.root.span.bytes() == b"carrier: Option<string>.None"))));
            for name in ["exported", "selfReplace"] {
                let report = lower(name, default);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                assert!(path_drops(mir, mir.entry).is_empty());
            }
            let replaced = lower("replace", default);
            let mir = replaced
                .mir()
                .expect("whole replacement cleans the old carrier, not the exported replacement");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::DropOptionPayload { operand, some }]
                if operand.root.span.bytes() == b"held" && *some == &option.some.id)
            );
            let ops = linear(mir, mir.entry);
            let replacement: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| match op {
                    Operation::Move { from, .. } => {
                        from.root.span.bytes() == b"incoming: Option<string>"
                    }
                    Operation::Cleanup(actions) => actions
                        .iter()
                        .any(|action| matches!(action, Obligation::DropOptionPayload { .. })),
                    Operation::Install { .. } | Operation::Export(_) => true,
                    _ => false,
                })
                .collect();
            assert!(
                matches!(replacement.as_slice(), [Operation::Move { into: Some(staged), .. }, Operation::Cleanup(_), Operation::Install { target, incoming }, Operation::Export(_)]
                if staged.root.node == incoming.root.node && target.root.span.bytes() == b"held"),
                "{replacement:?}"
            );
            let discarded = lower("discarded", default);
            let mir = discarded
                .mir()
                .expect("discard drops the moved carrier before continuing");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::DropOptionPayload { operand, some }]
                if operand.root.span.bytes() == b"carrier" && *some == &option.some.id)
            );
            let actions: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Move { from, .. } => {
                        from.root.span.bytes() == b"carrier: Option<string>"
                    }
                    Operation::Cleanup(actions) => actions
                        .iter()
                        .any(|action| matches!(action, Obligation::DropOptionPayload { .. })),
                    Operation::Call(_) => true,
                    _ => false,
                })
                .collect();
            assert!(
                matches!(actions.as_slice(), [Operation::Move { into: Some(staged), .. }, Operation::Cleanup(cleanup), Operation::Call(callee)]
                if *callee == id("afterDrop") && matches!(cleanup.as_slice(), [Obligation::DropOptionPayload { operand, .. }] if operand.root.node == staged.root.node)),
                "{actions:?}"
            );
            for (name, report) in [("conditional", conditional), ("discarded", discarded)] {
                let exact = AnalysisLimits {
                    max_work: report.statistics().work,
                    max_entries: report.statistics().entries,
                    ..default
                };
                assert!(lower(name, exact).mir().is_some());
                for short in [
                    AnalysisLimits {
                        max_work: exact.max_work - 1,
                        ..exact
                    },
                    AnalysisLimits {
                        max_entries: exact.max_entries - 1,
                        ..exact
                    },
                ] {
                    let limited = lower(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none());
                }
            }
        },
    );
}

#[test]
fn explicit_option_text_disposal_still_needs_a_typed_selection() {
    use compiler::analysis::scalars::ScalarUnsupported;
    use compiler::analysis::typed::TypedCompletion;

    probe_report(
        "propagation::explicit_option_text_disposal_still_needs_a_typed_selection",
        r#"
            import std::core::dispose as release;
            function disposeCarrier(carrier: Option<string>): void { release((carrier)); }
            function disposeNone(carrier: Option<string>.None): void { release(carrier); }
        "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let obligations = typed.bodies().obligations();
            assert_eq!(obligations.len(), 2);
            assert!(
                obligations
                    .iter()
                    .all(|o| o.reason == ScalarUnsupported::FamilySelection)
            );
            assert_eq!(
                obligations
                    .iter()
                    .map(|o| o.site.span.bytes())
                    .collect::<Vec<_>>(),
                [
                    b"release((carrier))".as_slice(),
                    b"release(carrier)".as_slice()
                ]
            );
        },
    );
}

#[test]
fn fresh_option_text_propagation_stages_the_constructor_once() {
    probe_with_typed(
        "propagation::fresh_option_text_propagation_stages_the_constructor_once",
        r#"
            function relay(owner: string): string { return owner; }
            function after(bit: bool): bool { return bit; }
            function named(owner: string): Option<bool> {
                let payload = (Option<string>.Some(relay(owner)))?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function discarded(owner: string): Option<bool> {
                Option<string>.Some(relay(owner))?;
                return Option.Some(after(true));
            }
            function empty(owner: string): Option<bool> {
                let _ = Option<string>.None?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function live(owner: string): bool {
                let held = Option<string>.Some(owner);
                return true;
            }
            function reused(owner: string): Option<bool> {
                let _ = Option<string>.Some(relay(owner))?;
                dispose(owner);
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let named = lower("named", default);
            let mir = named
                .mir()
                .unwrap_or_else(|| panic!("named: {:?}", named.completion()));
            assert_eq!(calls_on_path(mir, []), [id("relay"), id("after")]);
            let ops = linear(mir, mir.entry);
            assert_return(&ops, &option.some.id, true, &[b"payload"]);
            let (take, operand) = ops
                .iter()
                .enumerate()
                .find_map(|(i, op)| match op {
                    Operation::TakeOptionPayload { operand, into }
                        if into.root.span.bytes() == b"payload" =>
                    {
                        Some((i, *operand))
                    }
                    _ => None,
                })
                .expect("move the constructed Some payload into the named binding");
            let construction = ops.iter().position(|op| matches!(op, Operation::ConstructVariant { variant, payload: Some(payload), into }
                if *variant == &option.some.id && into.root.node == operand.root.node && payload.root.span.bytes() == b"relay(owner)")).unwrap();
            let relay = ops
                .iter()
                .position(|op| matches!(op, Operation::Call(callee) if *callee == id("relay")))
                .unwrap();
            let after = ops
                .iter()
                .position(|op| matches!(op, Operation::Call(callee) if *callee == id("after")))
                .unwrap();
            assert!(relay < construction && construction < take && take < after);
            let discarded = lower("discarded", default);
            let mir = discarded
                .mir()
                .expect("fresh Some does not need variant testing");
            assert_eq!(calls_on_path(mir, []), [id("relay"), id("after")]);
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(payload)]
                if payload.root.span.bytes() == b"Option<string>.Some(relay(owner))?")
            );
            let staged: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::TakeOptionPayload { .. } => true,
                    Operation::Cleanup(actions) => actions
                        .iter()
                        .any(|action| matches!(action, Obligation::Drop(_))),
                    Operation::Call(callee) => *callee == id("after"),
                    _ => false,
                })
                .collect();
            assert!(
                matches!(staged.as_slice(), [Operation::TakeOptionPayload { into, .. }, Operation::Cleanup(actions), Operation::Call(_)]
                if matches!(actions.as_slice(), [Obligation::Drop(payload)] if payload.root.node == into.root.node)),
                "{staged:?}"
            );
            let empty = lower("empty", default);
            let mir = empty.mir().expect("fresh None has only the failure edge");
            assert!(calls_on_path(mir, []).is_empty());
            let ops = linear(mir, mir.entry);
            assert_return(&ops, &option.none.id, false, &[b"owner: string"]);
            let none: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::ConstructVariant { .. }
                            | Operation::EndEmptyOption(_)
                            | Operation::TakeOptionPayload { .. }
                    )
                })
                .collect();
            assert!(
                matches!(none.as_slice(), [Operation::ConstructVariant { variant: input, payload: None, into }, Operation::EndEmptyOption(operand), Operation::ConstructVariant { variant: returned, payload: None, .. }]
                if *input == &option.none.id && *returned == &option.none.id && into.root.node == operand.root.node),
                "{none:?}"
            );
            let live = lower("live", default);
            let mir = live.mir().expect("fresh carrier joins normal cleanup");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::DropOptionPayload { operand, some }]
                if operand.root.span.bytes() == b"held" && *some == &option.some.id)
            );
            let reused = lower("reused", default);
            assert_eq!(reused.completion(), Completion::SourceRejected);
            assert!(reused.mir().is_none());
            let [finding] = reused.findings() else {
                panic!("expected one moved-source finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(owner)");
            assert_eq!(finding.related.span.bytes(), b"owner");
            let exact = AnalysisLimits {
                max_work: named.statistics().work,
                max_entries: named.statistics().entries,
                ..default
            };
            assert!(lower("named", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("named", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn option_text_factory_propagation_checks_the_reached_call_once() {
    probe_with_typed(
        "propagation::option_text_factory_propagation_checks_the_reached_call_once",
        r#"
            function stage(owner: string): string { return owner; }
            function after(bit: bool): bool { return bit; }
            function choose(flag: bool): bool { return flag; }
            function make(flag: bool, owner: string): Option<string> {
                if (flag) { return Option<string>.Some(owner); }
                return Option<string>.None;
            }
            function makeSome(owner: string): Option<string>.Some { return Option<string>.Some(owner); }
            function makeNone(owner: string): Option<string>.None { dispose(owner); return Option<string>.None; }
            function named(flag: bool, owner: string, later: string): Option<bool> {
                let output = ( ( ( (make(choose(flag), stage(owner))) ) ) )?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function flat(flag: bool, owner: string, later: string): Option<bool> {
                let output = make(choose(flag), stage(owner))?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function ordinary(owner: string): bool {
                let _ = makeSome(owner);
                return true;
            }
            function discarded(owner: string): Option<bool> {
                makeSome(owner)?;
                return Option.Some(after(true));
            }
            function empty(owner: string, later: string): Option<bool> {
                let _ = makeNone(owner)?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function invalid(owner: string): Option<string>.None {
                dispose(owner); dispose(owner); return Option<string>.None;
            }
            function reached(owner: string): Option<bool> {
                let _ = invalid(owner)?;
                return Option.Some(true);
            }
            function borrowedFactory(owner: &string): Option<string> { return Option<string>.None; }
            function excluded(owner: string): Option<bool> {
                let _ = borrowedFactory(&owner)?;
                return Option.Some(true);
            }
            function embedded(flag: Option<bool>, owner: string): Option<bool> {
                let _ = make(flag?, owner)?;
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let named = lower("named", default);
            let mir = named
                .mir()
                .unwrap_or_else(|| panic!("named factory: {:?}", named.completion()));
            let (yes, no) = variant_edges(
                mir,
                mir.entry,
                &option.some.id,
                b"( ( ( (make(choose(flag), stage(owner))) ) ) )",
            );
            let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| matches!(op, Operation::Call(_) | Operation::TestVariant { .. }))
                .collect();
            let [
                Operation::Call(choose),
                Operation::Call(stage),
                Operation::Call(make),
                Operation::TestVariant { operand, .. },
            ] = prefix.as_slice()
            else {
                panic!("expected two ordered stages, one factory call, then one test: {prefix:?}")
            };
            assert_eq!(
                [*choose, *stage, *make],
                [id("choose"), id("stage"), id("make")]
            );
            assert_eq!(
                calls_on_path(mir, [false]),
                [id("choose"), id("stage"), id("make")]
            );
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("choose"), id("stage"), id("make"), id("after")]
            );
            assert!(linear(mir, yes).iter().any(
                |op| matches!(op, Operation::TakeOptionPayload { operand: consumed, into }
                if consumed.root.node == operand.root.node && into.root.span.bytes() == b"output")
            ));
            assert!(linear(mir, no).iter().any(|op| matches!(op, Operation::EndEmptyOption(consumed) if consumed.root.node == operand.root.node)));
            assert_return(
                &linear(mir, yes),
                &option.some.id,
                true,
                &[b"output", b"later: string"],
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            let discarded = lower("discarded", default);
            let mir = discarded
                .mir()
                .expect("known Some factory runs, then drops its payload before the successor");
            assert_eq!(calls_on_path(mir, []), [id("makeSome"), id("after")]);
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(payload)]
                if payload.root.span.bytes() == b"makeSome(owner)?")
            );
            let ops: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Call(_) | Operation::TakeOptionPayload { .. } => true,
                    Operation::Cleanup(actions) => actions
                        .iter()
                        .any(|action| matches!(action, Obligation::Drop(_))),
                    _ => false,
                })
                .collect();
            assert!(
                matches!(ops.as_slice(), [Operation::Call(factory), Operation::TakeOptionPayload { into, .. }, Operation::Cleanup(actions), Operation::Call(after)]
                if *factory == id("makeSome") && *after == id("after") && matches!(actions.as_slice(), [Obligation::Drop(payload)] if payload.root.node == into.root.node)),
                "{ops:?}"
            );
            let empty = lower("empty", default);
            let mir = empty
                .mir()
                .expect("known None still calls the factory before returning");
            assert_eq!(calls_on_path(mir, []), [id("makeNone")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            let reached = lower("reached", default);
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("expected one reached factory finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(owner)");
            assert_eq!(finding.related.span.bytes(), b"dispose(owner)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let excluded = lower("excluded", default);
            assert_eq!(excluded.completion(), Completion::Incomplete);
            assert!(excluded.mir().is_none());
            assert_eq!(
                excluded.blocked_at().unwrap().span.bytes(),
                b"borrowedFactory(&owner)?"
            );
            let embedded = lower("embedded", default);
            assert_eq!(embedded.completion(), Completion::Incomplete);
            assert!(embedded.mir().is_none());
            let ordinary = lower("ordinary", default);
            assert_eq!(ordinary.completion(), Completion::Incomplete);
            assert!(ordinary.mir().is_none());
            assert_eq!(
                ordinary.blocked_at().unwrap().span.bytes(),
                b"makeSome(owner)"
            );
            let depth = AnalysisLimits {
                max_depth: named.statistics().max_depth - 1,
                ..default
            };
            assert!(lower("flat", depth).mir().is_some());
            let grouped = lower("named", depth);
            assert_eq!(grouped.completion(), Completion::ResourceLimit);
            assert!(grouped.mir().is_none());
            let exact = AnalysisLimits {
                max_work: named.statistics().work,
                max_entries: named.statistics().entries,
                ..default
            };
            assert!(lower("named", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("named", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn option_text_pipe_propagation_stages_input_and_transfers_the_returned_owner() {
    probe_with_typed(
        "propagation::option_text_pipe_propagation_stages_input_and_transfers_the_returned_owner",
        r#"
            function stage(owner: string): string { return owner; }
            function choose(flag: bool): bool { return flag; }
            function after(bit: bool): bool { return bit; }
            function make(flag: bool, owner: string): Option<string> {
                if (flag) { return Option<string>.Some(owner); }
                return Option<string>.None;
            }
            function makeSome(flag: bool, owner: string): Option<string>.Some { return Option<string>.Some(owner); }
            function makeNone(owner: string): Option<string>.None { dispose(owner); return Option<string>.None; }
            function named(owner: string, flag: bool, later: string): Option<bool> {
                let output = stage(owner) |> make(choose(flag), _)?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function grouped(owner: string, flag: bool, later: string): Option<bool> {
                let output = (((((stage(owner)))))) |> make(choose(flag), _)?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function absent(owner: string, later: string): Option<bool> {
                let _ = stage(owner) |> makeNone?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function combined(flag: Option<bool>, owner: string, later: string): Option<bool> {
                (flag?) |> makeSome(stage(owner))?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function terminal(flag: Option<bool>.None, owner: string): Option<bool> {
                let _ = (flag?) |> makeSome(stage(owner))?;
                return Option.Some(true);
            }
            function keep(flag: bool, owner: string): bool { dispose(owner); return flag; }
            function terminalScalar(flag: Option<bool>.None, owner: string): Option<bool> {
                let _ = (flag?) |> keep(stage(owner));
                return Option.Some(true);
            }
            function writtenFailure(owner: string, flag: Option<bool>.None): Option<bool> {
                let _ = stage(owner) |> invalid(flag?, _)?;
                return Option.Some(true);
            }
            function invalid(flag: bool, owner: string): Option<string>.None {
                dispose(owner); dispose(owner); return Option<string>.None;
            }
            function reached(owner: string): Option<bool> {
                let _ = owner |> invalid(true, _)?;
                return Option.Some(true);
            }
            function written(owner: string, flag: Option<bool>, later: string): Option<bool> {
                let output = stage(owner) |> make(flag?, _)?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function composite(input: Option<bool>, owner: string): Option<string> { return Option<string>.Some(owner); }
            function excluded(input: Option<bool>, owner: string): Option<bool> {
                let _ = input |> composite(owner)?;
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let named = lower("named", default);
            let mir = named
                .mir()
                .unwrap_or_else(|| panic!("named pipe: {:?}", named.completion()));
            let source = b"stage(owner) |> make(choose(flag), _)?";
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, source);
            let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| matches!(op, Operation::Call(_) | Operation::TestVariant { .. }))
                .collect();
            let [
                Operation::Call(stage),
                Operation::Call(choose),
                Operation::Call(make),
                Operation::TestVariant { operand, .. },
            ] = prefix.as_slice()
            else {
                panic!("expected input, written argument, step, then variant test: {prefix:?}")
            };
            assert_eq!(
                [*stage, *choose, *make],
                [id("stage"), id("choose"), id("make")]
            );
            assert!(linear(mir, yes).iter().any(
                |op| matches!(op, Operation::TakeOptionPayload { operand: carrier, into }
                if carrier.root.node == operand.root.node && into.root.span.bytes() == b"output")
            ));
            assert!(linear(mir, no).iter().any(|op| matches!(op, Operation::EndEmptyOption(carrier) if carrier.root.node == operand.root.node)));
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("choose"), id("make"), id("after")]
            );
            assert_eq!(
                calls_on_path(mir, [false]),
                [id("stage"), id("choose"), id("make")]
            );
            assert_return(
                &linear(mir, yes),
                &option.some.id,
                true,
                &[b"output", b"later: string"],
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            let absent = lower("absent", default);
            let mir = absent
                .mir()
                .expect("known None skips only the variant test and the continuation");
            assert_eq!(calls_on_path(mir, []), [id("stage"), id("makeNone")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            let combined = lower("combined", default);
            let mir = combined
                .mir()
                .expect("known Some drops the string only after the input continues");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"flag");
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("makeSome"), id("after")]
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"later: string", b"owner: string"],
            );
            assert!(
                matches!(path_drops(mir, yes).as_slice(), [Obligation::Drop(payload), Obligation::Drop(later)]
                if payload.root.span.bytes() == b"(flag?) |> makeSome(stage(owner))?" && later.root.span.bytes() == b"later: string")
            );
            let success: Vec<_> = linear(mir, yes).into_iter().filter(|op| match op {
                Operation::TakeOptionPayload { .. } => true,
                Operation::Cleanup(actions) => actions.iter().any(|action| matches!(action, Obligation::Drop(place) if place.root.span.bytes() == b"(flag?) |> makeSome(stage(owner))?")),
                Operation::Call(callee) => *callee == id("after"),
                _ => false,
            }).collect();
            assert!(
                matches!(success.as_slice(), [Operation::TakeOptionPayload { into, .. }, Operation::Cleanup(actions), Operation::Call(_)]
                if matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.node == into.root.node)),
                "{success:?}"
            );
            let terminal = lower("terminal", default);
            let mir = terminal
                .mir()
                .expect("known argument failure skips the carrier production");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"owner: string"],
            );
            let scalar = lower("terminalScalar", default);
            assert!(scalar.mir().is_some());
            assert_eq!(terminal.statistics().entries, scalar.statistics().entries);
            let written = lower("writtenFailure", default);
            let mir = written
                .mir()
                .expect("known written failure only drops the staged input");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(owner)"],
            );
            let both = lower("written", default);
            let mir = both
                .mir()
                .expect("written argument and step each have their own failure prefix");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"flag");
            let (step_yes, step_no) = variant_edges(
                mir,
                yes,
                &option.some.id,
                b"stage(owner) |> make(flag?, _)?",
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(owner)", b"later: string"],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            assert_return(
                &linear(mir, step_yes),
                &option.some.id,
                true,
                &[b"output", b"later: string"],
            );
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(calls_on_path(mir, [true, false]), [id("stage"), id("make")]);
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("stage"), id("make"), id("after")]
            );
            let reached = lower("reached", default);
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("expected one reached-step finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(owner)");
            assert_eq!(finding.related.span.bytes(), b"dispose(owner)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let excluded = lower("excluded", default);
            assert_eq!(excluded.completion(), Completion::Incomplete);
            assert!(excluded.mir().is_none());
            assert_eq!(
                excluded.blocked_at().unwrap().span.bytes(),
                b"input |> composite(owner)?"
            );
            let grouped = lower("grouped", default);
            assert!(grouped.mir().is_some());
            let depth = AnalysisLimits {
                max_depth: grouped.statistics().max_depth - 1,
                ..default
            };
            assert!(lower("named", depth).mir().is_some());
            let exhausted = lower("grouped", depth);
            assert_eq!(exhausted.completion(), Completion::ResourceLimit);
            assert!(exhausted.mir().is_none());
            let exact = AnalysisLimits {
                max_work: named.statistics().work,
                max_entries: named.statistics().entries,
                ..default
            };
            assert!(lower("named", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("named", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn owning_option_argument_propagation_cleans_only_reached_stages() {
    probe_with_typed(
        "propagation::owning_option_argument_propagation_cleans_only_reached_stages",
        r#"
            function stage(value: string): string { return value; }
            function useAll(before: string, value: string, bit: bool): bool {
                dispose(value); dispose(before); return bit;
            }
            function useLate(before: string, bit: bool, value: string): bool {
                dispose(value); dispose(before); return bit;
            }
            function make(value: string): Option<string>.Some { return Option<string>.Some(value); }
            function paired(before: string, input: Option<string>, later: Option<bool>, keep: string): Option<bool> {
                let bit = useAll(stage(before), input?, later?);
                return Option.Some(bit);
            }
            function knownLater(before: string, input: Option<string>.Some, later: Option<bool>.None): Option<bool> {
                let _ = useAll(stage(before), input?, later?);
                return Option.Some(true);
            }
            function skipped(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = useLate(stage(before), gate?, input?);
                return Option.Some(true);
            }
            function noneFirst(before: string, input: Option<string>.None, later: Option<bool>): Option<bool> {
                let _ = useAll(stage(before), input?, later?);
                return Option.Some(true);
            }
            function factory(before: string, value: string, gate: Option<bool>): Option<bool> {
                let bit = useAll(stage(before), make(value)?, gate?);
                return Option.Some(bit);
            }
            function piped(before: string, value: string, gate: Option<bool>): Option<bool> {
                let bit = useAll(stage(before), (value |> make?), gate?);
                return Option.Some(bit);
            }
            function freshPipe(before: string, value: string, gate: Option<bool>): Option<bool> {
                let bit = (Option<string>.Some(stage(value))?) |> useLate(stage(before), gate?, _);
                return Option.Some(bit);
            }
            function deeper(before: string, input: Option<string>, flag: bool): Option<bool> {
                let bit = useAll(stage(before), stage(input?), flag);
                return Option.Some(bit);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let paired = lower("paired", default);
            let mir = paired
                .mir()
                .unwrap_or_else(|| panic!("paired: {:?}", paired.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (both, later_no) = variant_edges(mir, yes, &option.some.id, b"later");
            let prefix = straight_blocks(mir, yes);
            let transferred = prefix
                .iter()
                .find_map(|block| match &block.operation {
                    Operation::TakeOptionPayload { operand, into }
                        if operand.root.span.bytes() == b"input"
                            && into.root.span.bytes() == b"input?" =>
                    {
                        Some(into.root.node)
                    }
                    _ => None,
                })
                .expect("stage the owning payload before checking the later argument");
            assert!(
                matches!(path_drops(mir, later_no).first(), Some(Obligation::Drop(place)) if place.root.node == transferred)
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(before)", b"keep: string"],
            );
            assert_return(
                &linear(mir, later_no),
                &option.none.id,
                false,
                &[b"input?", b"stage(before)", b"keep: string"],
            );
            assert_return(
                &linear(mir, both),
                &option.some.id,
                true,
                &[b"keep: string"],
            );
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(calls_on_path(mir, [true, false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("stage"), id("useAll")]
            );
            let known = lower("knownLater", default);
            let mir = known
                .mir()
                .expect("a reached Some string needs cleanup even when the later exit is certain");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"input?", b"stage(before)"],
            );
            let skipped = lower("skipped", default);
            let mir = skipped
                .mir()
                .expect("a skipped owning argument is still a live lexical carrier");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(staged), Obligation::DropOptionPayload { operand, some }]
                if staged.root.span.bytes() == b"stage(before)" && operand.root.span.bytes() == b"input: Option<string>" && *some == &option.some.id)
            );
            let ops = linear(mir, mir.entry);
            let export = ops
                .iter()
                .position(|op| matches!(op, Operation::Export(_)))
                .unwrap();
            assert!(ops[..export].iter().all(|op| !matches!(
                op,
                Operation::Cleanup(_) | Operation::TakeOptionPayload { .. } | Operation::Drop(_)
            )));
            let none = lower("noneFirst", default);
            let mir = none
                .mir()
                .expect("known None skips the owning success slot and the suffix");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            let ops = linear(mir, mir.entry);
            assert_return(&ops, &option.none.id, false, &[b"stage(before)"]);
            assert!(ops.iter().any(|op| matches!(op, Operation::EndEmptyOption(carrier) if carrier.root.span.bytes() == b"input")));
            assert!(
                ops.iter()
                    .all(|op| !matches!(op, Operation::TakeOptionPayload { .. }))
            );
            for (name, arg) in [
                ("factory", b"make(value)?".as_slice()),
                ("piped", b"(value |> make?)".as_slice()),
            ] {
                let report = lower(name, default);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"gate");
                assert_return(
                    &linear(mir, no),
                    &option.none.id,
                    false,
                    &[arg, b"stage(before)"],
                );
                assert_return(&linear(mir, yes), &option.some.id, true, &[]);
                assert_eq!(calls_on_path(mir, [false]), [id("stage"), id("make")]);
                assert_eq!(
                    calls_on_path(mir, [true]),
                    [id("stage"), id("make"), id("useAll")]
                );
            }
            let fresh = lower("freshPipe", default);
            let mir = fresh.mir().expect(
                "fresh owning input stages before written arguments even with a late placeholder",
            );
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"gate");
            let prefix_calls: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .filter_map(|block| {
                    matches!(block.operation, Operation::Call(_)).then_some(block.site.span.bytes())
                })
                .collect();
            assert_eq!(
                prefix_calls,
                [b"stage(value)".as_slice(), b"stage(before)".as_slice()]
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(before)", b"(Option<string>.Some(stage(value))?)"],
            );
            assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("stage"), id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("stage"), id("useLate")]
            );
            let deeper = lower("deeper", default);
            assert_eq!(deeper.completion(), Completion::Incomplete);
            assert!(deeper.mir().is_none());
            let exact = AnalysisLimits {
                max_work: paired.statistics().work,
                max_entries: paired.statistics().entries,
                ..default
            };
            assert!(lower("paired", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("paired", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn owning_option_arguments_before_propagating_steps_have_distinct_exits() {
    probe_with_typed(
        "propagation::owning_option_arguments_before_propagating_steps_have_distinct_exits",
        r#"
            function stage(value: string): string { return value; }
            function finish(before: string, bit: bool, value: string): Option<string> {
                dispose(before);
                if (bit) { return Option<string>.Some(value); }
                return Option<string>.None;
            }
            function after(bit: bool): bool { return bit; }
            function composed(input: Option<string>, before: string, gate: Option<bool>, later: string): Option<bool> {
                let output = (input?) |> finish(stage(before), gate?, _)?;
                let bit = after(true);
                return Option.Some(bit);
            }
            function invalid(bit: bool, value: string, before: string): Option<bool> {
                dispose(before); dispose(before); dispose(value); return Option.Some(bit);
            }
            function skipped(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = stage(before) |> invalid(gate?, input?, _)?;
                return Option.Some(true);
            }
            function reached(before: string, input: Option<string>.Some): Option<bool> {
                let bit = stage(before) |> invalid(true, input?, _)?;
                return Option.Some(bit);
            }
            function scalar(value: string): Option<bool>.Some { dispose(value); return Option<bool>.Some(true); }
            function consume(before: string, bit: bool, value: string): bool { dispose(value); dispose(before); return bit; }
            function terminal(input: Option<string>.None, before: string, gate: Option<bool>): Option<bool> {
                let _ = (input?) |> finish(stage(before), gate?, _)?;
                return Option.Some(true);
            }
            function ordinaryTerminal(input: Option<string>.None, before: string, gate: Option<bool>): Option<bool> {
                let _ = (input?) |> consume(stage(before), gate?, _);
                return Option.Some(true);
            }
            function statement(input: Option<string>): Option<bool> {
                (input?) |> scalar?;
                return Option.Some(after(true));
            }
            function written(before: string, bit: bool, input: Option<string>, later: string): Option<bool> {
                stage(before) |> finish(bit, input?)?;
                let next = after(true);
                return Option.Some(next);
            }
            function returnSome(value: string): Option<string>.Some { return Option<string>.Some(value); }
            function skippedDeeper(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = stage(before) |> invalid(gate?, ((input?) |> returnSome?), _)?;
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let composed = lower("composed", default);
            let mir = composed
                .mir()
                .unwrap_or_else(|| panic!("composed owning input: {:?}", composed.completion()));
            let (input_yes, input_no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (gate_yes, gate_no) = variant_edges(mir, input_yes, &option.some.id, b"gate");
            let (step_yes, step_no) = variant_edges(
                mir,
                gate_yes,
                &option.some.id,
                b"(input?) |> finish(stage(before), gate?, _)?",
            );
            let prefix: Vec<_> = straight_blocks(mir, input_yes)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| {
                    matches!(
                        op,
                        Operation::TakeOptionPayload { .. }
                            | Operation::Call(_)
                            | Operation::TestVariant { .. }
                    )
                })
                .collect();
            let [
                Operation::TakeOptionPayload { operand, into },
                Operation::Call(stage),
                Operation::TestVariant { .. },
            ] = prefix.as_slice()
            else {
                panic!("expected input transfer before written arguments: {prefix:?}")
            };
            assert_eq!(operand.root.span.bytes(), b"input");
            assert_eq!(into.root.span.bytes(), b"(input?)");
            assert_eq!(*stage, id("stage"));
            assert!(
                matches!(path_drops(mir, gate_no).as_slice(), [Obligation::Drop(_), Obligation::Drop(staged), Obligation::Drop(_)]
                if staged.root.node == into.root.node)
            );
            assert_return(
                &linear(mir, input_no),
                &option.none.id,
                false,
                &[b"later: string", b"before: string"],
            );
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[b"stage(before)", b"(input?)", b"later: string"],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            assert_return(
                &linear(mir, step_yes),
                &option.some.id,
                true,
                &[b"output", b"later: string"],
            );
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(calls_on_path(mir, [true, false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, true, false]),
                [id("stage"), id("finish")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true]),
                [id("stage"), id("finish"), id("after")]
            );
            let terminal = lower("terminal", default);
            let mir = terminal
                .mir()
                .expect("known empty input never stages the written arguments or the step carrier");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"before: string"],
            );
            let ordinary = lower("ordinaryTerminal", default);
            assert!(ordinary.mir().is_some());
            assert_eq!(terminal.statistics().entries, ordinary.statistics().entries);
            let skipped = lower("skipped", default);
            let mir = skipped
                .mir()
                .expect("known earlier failure skips the owning argument and invalid step");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(stage), Obligation::DropOptionPayload { operand, some }]
                if stage.root.span.bytes() == b"stage(before)" && operand.root.span.bytes() == b"input: Option<string>" && *some == &option.some.id)
            );
            let reached = lower("reached", default);
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("expected a reached-step double disposal")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(before)");
            assert_eq!(finding.related.span.bytes(), b"dispose(before)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let statement = lower("statement", default);
            let mir = statement
                .mir()
                .expect("an owning input does not change the scalar statement result rule");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(calls_on_path(mir, [true]), [id("scalar"), id("after")]);
            assert_return(&linear(mir, no), &option.none.id, false, &[]);
            assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            let written = lower("written", default);
            let mir = written
                .mir()
                .expect("a written owning argument and statement result stage independently");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let source = b"stage(before) |> finish(bit, input?)?";
            let (step_yes, step_no) = variant_edges(mir, yes, &option.some.id, source);
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(before)", b"later: string"],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[b"later: string"],
            );
            assert!(linear(mir, step_no).iter().any(|op| matches!(op, Operation::EndEmptyOption(carrier) if carrier.root.span.bytes() == source)));
            assert!(
                matches!(path_drops(mir, step_yes).as_slice(), [Obligation::Drop(output), Obligation::Drop(later)]
                if output.root.span.bytes() == source && later.root.span.bytes() == b"later: string")
            );
            let order: Vec<_> = linear(mir, step_yes).into_iter().filter(|op| match op {
                Operation::TakeOptionPayload { .. } | Operation::Call(_) => true,
                Operation::Cleanup(actions) => actions.iter().any(|action| matches!(action, Obligation::Drop(place) if place.root.span.bytes() == source)),
                _ => false,
            }).collect();
            assert!(
                matches!(order.as_slice(), [Operation::TakeOptionPayload { into, .. }, Operation::Cleanup(actions), Operation::Call(callee)]
                if *callee == id("after") && matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.node == into.root.node)),
                "{order:?}"
            );
            let skipped_deeper = lower("skippedDeeper", default);
            assert_eq!(skipped_deeper.completion(), Completion::Incomplete);
            assert!(skipped_deeper.mir().is_none());
            assert_eq!(
                skipped_deeper.blocked_at().unwrap().span.bytes(),
                b"stage(before) |> invalid(gate?, ((input?) |> returnSome?), _)?"
            );
            let exact = AnalysisLimits {
                max_work: composed.statistics().work,
                max_entries: composed.statistics().entries,
                ..default
            };
            assert!(lower("composed", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("composed", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn one_composed_pipe_argument_keeps_inner_and_outer_cleanup_prefixes() {
    probe_with_typed(
        "propagation::one_composed_pipe_argument_keeps_inner_and_outer_cleanup_prefixes",
        r#"
            function stage(value: string): string { return value; }
            function finish(inside: string, bit: bool, value: string): Option<string> {
                dispose(inside);
                if (bit) { return Option<string>.Some(value); }
                return Option<string>.None;
            }
            function useAll(before: string, value: string, bit: bool): bool {
                dispose(value); dispose(before); return bit;
            }
            function nested(before: string, input: Option<string>, inside: string, gate: Option<bool>, later: Option<bool>, keep: string): Option<bool> {
                let bit = useAll(stage(before), ((input?) |> finish(stage(inside), gate?, _)?), later?);
                return Option.Some(bit);
            }
            function terminal(before: string, input: Option<string>.None, inside: string, gate: Option<bool>, later: Option<bool>): Option<bool> {
                let _ = useAll(stage(before), ((input?) |> finish(stage(inside), gate?, _)?), later?);
                return Option.Some(true);
            }
            function always(value: string): Option<string>.Some { return Option<string>.Some(value); }
            function alwaysStep(before: string, input: Option<string>): Option<bool> {
                let bit = useAll(stage(before), ((input?) |> always?), true);
                return Option.Some(bit);
            }
            function bounce(value: string): Option<string> { return Option<string>.Some(value); }
            function tooDeep(before: string, input: Option<string>): Option<bool> {
                let bit = useAll(stage(before), ((((input?) |> bounce?) |> bounce?)), true);
                return Option.Some(bit);
            }
            function laterString(before: string, bit: bool, value: string): bool {
                dispose(value); dispose(before); return bit;
            }
            function skipped(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = laterString(stage(before), gate?, ((input?) |> bounce?));
                return Option.Some(true);
            }
            function twoPlain(left: string, right: string): Option<bool> {
                let bit = useAll((left |> always?), (right |> always?), true);
                return Option.Some(bit);
            }
            function invalidInner(inside: string, bit: bool, value: string): Option<string>.None {
                dispose(inside); dispose(inside); dispose(value); return Option<string>.None;
            }
            function invalidOuter(before: string, value: string, bit: bool): bool {
                dispose(before); dispose(before); dispose(value); return bit;
            }
            function terminalWritten(before: string, input: Option<string>.Some, inside: string, gate: Option<bool>.None): Option<bool> {
                let _ = invalidOuter(stage(before), ((input?) |> invalidInner(stage(inside), gate?, _)?), true);
                return Option.Some(true);
            }
            function invalidReached(before: string, input: Option<string>.Some, inside: string, gate: Option<bool>.Some): Option<bool> {
                let _ = invalidOuter(stage(before), ((input?) |> invalidInner(stage(inside), gate?, _)?), true);
                return Option.Some(true);
            }
            function stop(): never { return stop(); }
            function round(bit: bool): Option<bool> { return Option.Some(bit); }
            function both(first: bool, second: bool): bool { return second; }
            function skippedPlain(gate: Option<bool>.None, bit: bool): Option<bool> {
                let _ = both(gate?, ((if (bit) true else stop()) |> round?));
                return Option.Some(true);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let nested = lower("nested", default);
            let mir = nested
                .mir()
                .unwrap_or_else(|| panic!("nested pipe argument: {:?}", nested.completion()));
            let (input_yes, input_no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (gate_yes, gate_no) = variant_edges(mir, input_yes, &option.some.id, b"gate");
            let (step_yes, step_no) = variant_edges(
                mir,
                gate_yes,
                &option.some.id,
                b"(input?) |> finish(stage(inside), gate?, _)?",
            );
            let (all_yes, later_no) = variant_edges(mir, step_yes, &option.some.id, b"later");
            assert_return(
                &linear(mir, input_no),
                &option.none.id,
                false,
                &[b"stage(before)", b"keep: string", b"inside: string"],
            );
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[
                    b"stage(inside)",
                    b"(input?)",
                    b"stage(before)",
                    b"keep: string",
                ],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[b"stage(before)", b"keep: string"],
            );
            assert_return(
                &linear(mir, later_no),
                &option.none.id,
                false,
                &[
                    b"((input?) |> finish(stage(inside), gate?, _)?)",
                    b"stage(before)",
                    b"keep: string",
                ],
            );
            assert_return(
                &linear(mir, all_yes),
                &option.some.id,
                true,
                &[b"keep: string"],
            );
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("stage"), id("stage")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, false]),
                [id("stage"), id("stage"), id("finish")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true, true]),
                [id("stage"), id("stage"), id("finish"), id("useAll")]
            );
            let terminal = lower("terminal", default);
            let mir = terminal.mir().expect(
                "known failure inside the argument skips both calls and the later argument",
            );
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(before)", b"inside: string"],
            );
            let known_step = lower("alwaysStep", default);
            let mir = known_step
                .mir()
                .expect("a success-only step can still return through its argument");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(before)"],
            );
            assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("always"), id("useAll")]
            );
            let plain = lower("twoPlain", default);
            let mir = plain
                .mir()
                .expect("plain propagating pipes are not counted as compositions");
            assert_eq!(
                calls_on_path(mir, []),
                [id("always"), id("always"), id("useAll")]
            );
            assert_return(&linear(mir, mir.entry), &option.some.id, true, &[]);
            let terminal_written = lower("terminalWritten", default);
            let mir = terminal_written
                .mir()
                .expect("known written failure skips both invalid callees");
            assert_eq!(calls_on_path(mir, []), [id("stage"), id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(inside)", b"(input?)", b"stage(before)"],
            );
            let invalid = lower("invalidReached", default);
            assert_eq!(invalid.completion(), Completion::SourceRejected);
            assert!(invalid.mir().is_none());
            let [finding] = invalid.findings() else {
                panic!("expected a reached inner-step finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(inside)");
            assert_eq!(finding.related.span.bytes(), b"dispose(inside)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let skipped_plain = lower("skippedPlain", default);
            let skipped_plain_mir = skipped_plain
                .mir()
                .unwrap_or_else(|| panic!("skippedPlain: {:?}", skipped_plain.completion()));
            assert!(calls_on_path(skipped_plain_mir, []).is_empty());
            assert_return(
                &linear(skipped_plain_mir, skipped_plain_mir.entry),
                &option.none.id,
                false,
                &[],
            );
            for name in ["tooDeep", "skipped"] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
            }
            let exact = AnalysisLimits {
                max_work: nested.statistics().work,
                max_entries: nested.statistics().entries,
                ..default
            };
            assert!(lower("nested", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("nested", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn sibling_composed_arguments_keep_their_own_cleanup_prefixes() {
    probe_with_typed(
        "propagation::sibling_composed_arguments_keep_their_own_cleanup_prefixes",
        r#"
            function stage(value: string): string { return value; }
            function first(value: string): Option<string>.Some { return Option<string>.Some(value); }
            function second(inside: string, bit: bool, value: string): Option<string> {
                dispose(inside);
                if (bit) { return Option<string>.Some(value); }
                return Option<string>.None;
            }
            function useAll(left: string, right: string, bit: bool): bool {
                dispose(right); dispose(left); return bit;
            }
            function last(tail: string): bool { dispose(tail); return true; }
            function siblings(left: Option<string>, right: Option<string>, inside: string, gate: Option<bool>, tail: string): Option<bool> {
                let bit = useAll(((left?) |> first?), ((right?) |> second(stage(inside), gate?, _)?), last(tail));
                return Option.Some(bit);
            }
            function terminal(left: Option<string>.Some, right: Option<string>.Some, inside: string, gate: Option<bool>.None, tail: string): Option<bool> {
                let _ = useAll(((left?) |> first?), ((right?) |> second(stage(inside), gate?, _)?), last(tail));
                return Option.Some(true);
            }
            function skipped(left: Option<string>.None, right: Option<string>): Option<bool> {
                let _ = useAll(((left?) |> first?), ((right?) |> first?), true);
                return Option.Some(true);
            }
            function rejected(value: string): Option<string>.Some {
                dispose(value); dispose(value); return Option<string>.Some("never reached safely");
            }
            function invalid(left: Option<string>.Some, right: Option<string>.Some): Option<bool> {
                let bit = useAll(((left?) |> first?), ((right?) |> rejected?), true);
                return Option.Some(bit);
            }
            function useThree(left: string, middle: string, right: string): bool {
                dispose(right); dispose(middle); dispose(left); return true;
            }
            function widthTwo(left: Option<string>, middle: Option<string>, right: Option<string>, fallback: string): Option<bool> {
                let bit = useThree(((left?) |> first?), ((middle?) |> first?), fallback);
                return Option.Some(bit);
            }
            function widthThree(left: Option<string>, middle: Option<string>, right: Option<string>, fallback: string): Option<bool> {
                let bit = useThree(((left?) |> first?), ((middle?) |> first?), ((right?) |> first?));
                return Option.Some(bit);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let siblings = lower("siblings", default);
            let mir = siblings
                .mir()
                .unwrap_or_else(|| panic!("siblings: {:?}", siblings.completion()));
            let (left_yes, left_no) = variant_edges(mir, mir.entry, &option.some.id, b"left");
            let (right_yes, right_no) = variant_edges(mir, left_yes, &option.some.id, b"right");
            let (gate_yes, gate_no) = variant_edges(mir, right_yes, &option.some.id, b"gate");
            let (step_yes, step_no) = variant_edges(
                mir,
                gate_yes,
                &option.some.id,
                b"(right?) |> second(stage(inside), gate?, _)?",
            );
            let left_stage = b"((left?) |> first?)";
            let staged = straight_blocks(mir, left_yes)
                .into_iter()
                .find_map(|block| match &block.operation {
                    Operation::TakeOptionPayload { into, .. }
                        if into.root.span.bytes() == left_stage =>
                    {
                        Some(into.root.node)
                    }
                    _ => None,
                })
                .expect("first result is staged before testing the second input");
            assert!(
                matches!(path_drops(mir, gate_no).get(2), Some(Obligation::Drop(place)) if place.root.node == staged)
            );
            let first_failure = linear(mir, left_no);
            let (export, output) = first_failure
                .iter()
                .enumerate()
                .find_map(|(i, op)| match op {
                    Operation::Export(place) => Some((i, place)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(first_failure[..export].iter().filter(|op| matches!(op, Operation::ConstructVariant { variant, payload: None, into }
                if *variant == &option.none.id && into.root.node == output.root.node)).count(), 1);
            assert!(
                first_failure[..export]
                    .iter()
                    .all(|op| !matches!(op, Operation::Drop(_) | Operation::Cleanup(_)))
            );
            assert!(
                matches!(path_drops(mir, left_no).as_slice(), [Obligation::Drop(tail), Obligation::Drop(inside), Obligation::DropOptionPayload { operand, some }]
                if tail.root.span.bytes() == b"tail: string" && inside.root.span.bytes() == b"inside: string"
                && operand.root.span.bytes() == b"right: Option<string>" && *some == &option.some.id)
            );
            assert_return(
                &linear(mir, right_no),
                &option.none.id,
                false,
                &[left_stage, b"tail: string", b"inside: string"],
            );
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[b"stage(inside)", b"(right?)", left_stage, b"tail: string"],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[left_stage, b"tail: string"],
            );
            assert_return(&linear(mir, step_yes), &option.some.id, true, &[]);
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(calls_on_path(mir, [true, false]), [id("first")]);
            assert_eq!(
                calls_on_path(mir, [true, true, false]),
                [id("first"), id("stage")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true, false]),
                [id("first"), id("stage"), id("second")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true, true]),
                [
                    id("first"),
                    id("stage"),
                    id("second"),
                    id("last"),
                    id("useAll")
                ]
            );
            let terminal = lower("terminal", default);
            let mir = terminal.mir().expect(
                "known failure in the second sibling skips the step, suffix, and outer call",
            );
            assert_eq!(calls_on_path(mir, []), [id("first"), id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(inside)", b"(right?)", left_stage, b"tail: string"],
            );
            let skipped = lower("skipped", default);
            assert_eq!(skipped.completion(), Completion::Incomplete);
            assert!(skipped.mir().is_none());
            assert_eq!(
                skipped.blocked_at().unwrap().span.bytes(),
                b"useAll(((left?) |> first?), ((right?) |> first?), true)"
            );
            let invalid = lower("invalid", default);
            assert_eq!(invalid.completion(), Completion::SourceRejected);
            assert!(invalid.mir().is_none());
            let [finding] = invalid.findings() else {
                panic!("expected the second sibling's reached-callee finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(value)");
            assert_eq!(finding.related.span.bytes(), b"dispose(value)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let narrow = lower("widthTwo", default);
            assert!(narrow.mir().is_some());
            let wide = lower("widthThree", default);
            let mir = wide
                .mir()
                .expect("three siblings do not add a nesting layer");
            let (yes, _) = variant_edges(mir, mir.entry, &option.some.id, b"left");
            let (yes, _) = variant_edges(mir, yes, &option.some.id, b"middle");
            let (_, no) = variant_edges(mir, yes, &option.some.id, b"right");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"((middle?) |> first?)", left_stage, b"fallback: string"],
            );
            assert_eq!(
                calls_on_path(mir, [true, true, false]),
                [id("first"), id("first")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true]),
                [id("first"), id("first"), id("first"), id("useThree")]
            );
            assert!(wide.statistics().work > narrow.statistics().work);
            assert!(wide.statistics().entries > narrow.statistics().entries);
            assert_eq!(wide.statistics().max_depth, narrow.statistics().max_depth);
            let exact = AnalysisLimits {
                max_work: siblings.statistics().work,
                max_entries: siblings.statistics().entries,
                ..default
            };
            assert!(lower("siblings", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("siblings", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn composed_arguments_of_ordinary_pipes_keep_evaluation_order_and_cleanup() {
    probe_with_typed(
        "propagation::composed_arguments_of_ordinary_pipes_keep_evaluation_order_and_cleanup",
        r#"
            function stage(value: string): string { return value; }
            function finish(inside: string, bit: bool, value: string): Option<string> {
                dispose(inside);
                if (bit) { return Option<string>.Some(value); }
                return Option<string>.None;
            }
            function present(value: string): Option<string>.Some { return Option<string>.Some(value); }
            function unknown(value: string): Option<string> { return Option<string>.Some(value); }
            function useAll(value: string, bit: bool, prefix: string): bool {
                dispose(value); dispose(prefix); return bit;
            }
            function last(tail: string): bool { dispose(tail); return true; }
            function written(outer: string, input: Option<string>, inside: string, gate: Option<bool>, tail: string): Option<bool> {
                let bit = stage(outer) |> useAll(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _);
                return Option.Some(bit);
            }
            function inputPosition(input: Option<string>, between: string, later: Option<bool>): Option<bool> {
                let bit = ((input?) |> present?) |> useAll(stage(between), later?, _);
                return Option.Some(bit);
            }
            function terminal(outer: string, input: Option<string>.None, inside: string, gate: Option<bool>, tail: string): Option<bool> {
                let _ = stage(outer) |> useAll(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _);
                return Option.Some(true);
            }
            function outerStep(value: string, bit: bool, prefix: string): Option<bool> {
                dispose(value); dispose(prefix); return Option.Some(bit);
            }
            function excluded(outer: string, input: Option<string>): Option<bool> {
                let bit = outer |> outerStep(((input?) |> unknown?), true, _)?;
                return Option.Some(bit);
            }
        "#,
        |_, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower = |name, limits| lower_body(receipt, module, id(name), limits).unwrap();
            let default = AnalysisLimits::default();
            let option = typed.option_role().unwrap();
            let written = lower("written", default);
            let mir = written
                .mir()
                .unwrap_or_else(|| panic!("written composition: {:?}", written.completion()));
            let before_test: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .filter_map(|block| {
                    matches!(block.operation, Operation::Call(_)).then_some(block.site.span.bytes())
                })
                .collect();
            assert_eq!(before_test, [b"stage(outer)".as_slice()]);
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (gate_yes, gate_no) = variant_edges(mir, yes, &option.some.id, b"gate");
            let (step_yes, step_no) = variant_edges(
                mir,
                gate_yes,
                &option.some.id,
                b"(input?) |> finish(stage(inside), gate?, _)?",
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(outer)", b"tail: string", b"inside: string"],
            );
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[
                    b"stage(inside)",
                    b"(input?)",
                    b"stage(outer)",
                    b"tail: string",
                ],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[b"stage(outer)", b"tail: string"],
            );
            assert_return(&linear(mir, step_yes), &option.some.id, true, &[]);
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("stage"), id("stage")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, false]),
                [id("stage"), id("stage"), id("finish")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true]),
                [
                    id("stage"),
                    id("stage"),
                    id("finish"),
                    id("last"),
                    id("useAll")
                ]
            );
            let input = lower("inputPosition", default);
            let mir = input
                .mir()
                .expect("the composed input stages first despite its late formal slot");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (later_yes, later_no) = variant_edges(mir, yes, &option.some.id, b"later");
            let transferred = straight_blocks(mir, yes)
                .into_iter()
                .find_map(|block| match &block.operation {
                    Operation::TakeOptionPayload { into, .. }
                        if into.root.span.bytes() == b"((input?) |> present?)" =>
                    {
                        Some(into.root.node)
                    }
                    _ => None,
                })
                .expect("stage the composed input result before the later argument test");
            assert!(
                matches!(path_drops(mir, later_no).as_slice(), [Obligation::Drop(_), Obligation::Drop(staged)] if staged.root.node == transferred)
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"between: string"],
            );
            assert_return(
                &linear(mir, later_no),
                &option.none.id,
                false,
                &[b"stage(between)", b"((input?) |> present?)"],
            );
            assert_return(&linear(mir, later_yes), &option.some.id, true, &[]);
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("present"), id("stage")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("present"), id("stage"), id("useAll")]
            );
            let stopped = lower("terminal", default);
            let mir = stopped
                .mir()
                .expect("known inner failure skips the inner and outer steps and the suffix");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(outer)", b"tail: string", b"inside: string"],
            );
            let excluded = lower("excluded", default);
            assert_eq!(excluded.completion(), Completion::Incomplete);
            assert!(excluded.mir().is_none());
            assert_eq!(excluded.blocked_at().unwrap().span.bytes(), b"(input?)");
            let exact = AnalysisLimits {
                max_work: written.statistics().work,
                max_entries: written.statistics().entries,
                ..default
            };
            assert!(lower("written", exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = lower("written", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

fn variant_edges(
    mir: &Mir<'_>,
    mut pc: usize,
    expected: &loader::DeclarationId,
    source: &[u8],
) -> (usize, usize) {
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        match (&block.operation, block.terminator) {
            (Operation::TestVariant { operand, success }, Terminator::Branch { yes, no }) => {
                assert_eq!(operand.root.span.bytes(), source);
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
