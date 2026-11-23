use super::*;

#[path = "propagation/option_caller.rs"]
mod option_caller;
#[path = "propagation/result_text.rs"]
mod result_text;
#[path = "propagation/staging.rs"]
mod staging;

#[test]
fn owned_option_postfix_preserves_shared_text_backing() {
    probe_with_typed(
        "propagation::owned_option_postfix_preserves_shared_text_backing",
        r#"
            function forward(input: Option<&string>, tail: string): Option<&string> {
                let refValue = input?;
                return Option.Some(refValue);
            }
            function escaped(local: string): Option<&string> {
                let input: Option<&string> = Option.Some(&local);
                let refValue = input?;
                return Option.Some(refValue);
            }
            function ended(local: string): Option<bool> {
                let input: Option<&string> = Option.Some(&local);
                let _ = input?;
                dispose(local);
                return Option.Some(true);
            }
            function releaseLive(local: string): Option<&string> {
                let input: Option<&string> = Option.Some(&local);
                let refValue = input?;
                dispose(local);
                return Option.Some(refValue);
            }
        "#,
        |request, module, receipt, typed| {
            use compiler::analysis::ownership::Place;
            let check = |name, limits| {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &function.id, limits).unwrap()
            };
            let lower = |name| check(name, AnalysisLimits::default());
            let option = typed.option_role().unwrap();
            let same = |left: Place<'_>, right: Place<'_>| {
                left.projection.is_none()
                    && right.projection.is_none()
                    && left.root.node == right.root.node
            };
            let forward = lower("forward");
            assert_eq!(
                forward.completion(),
                Completion::Checked,
                "test_mode={}; blocked at {:?}",
                request.test_mode,
                forward.blocked_at().map(|s| s.span.bytes())
            );
            let mir = forward.mir().unwrap();
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let prefix = straight_blocks(mir, mir.entry);
            let [moved, test] = prefix.as_slice() else {
                panic!("stage one carrier and branch on it");
            };
            let Operation::Copy {
                from,
                into: Some(carrier),
            } = moved.operation
            else {
                panic!("copy the eligible carrier; consume only the stage");
            };
            assert_eq!(from.root.span.bytes(), b"input: Option<&string>");
            assert!(!same(from, carrier));
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, .. }
                if same(operand, carrier))
            );
            let success = linear(mir, yes);
            let [
                Operation::TakeOptionPayload {
                    operand,
                    into: binding,
                },
                Operation::Copy {
                    from,
                    into: Some(descriptor),
                },
                Operation::ConstructVariant {
                    variant,
                    payload: Some(payload),
                    into,
                },
                Operation::Export(out),
                ..,
            ] = success.as_slice()
            else {
                panic!("take, copy and reconstruct the shared descriptor before export");
            };
            assert!(same(*operand, carrier));
            assert_eq!(binding.root.span.bytes(), b"refValue");
            assert!(same(*from, *binding) && same(*descriptor, *payload) && same(*into, *out));
            assert_eq!(*variant, &option.some.id);
            assert_return(&success, &option.some.id, true, &[b"tail: string"]);
            let failure = linear(mir, no);
            assert!(matches!(failure.as_slice(),
                [Operation::EndEmptyOption(operand), Operation::ConstructVariant { payload: None, .. },
                 Operation::Export(_), ..] if same(*operand, carrier)));
            assert_return(&failure, &option.none.id, false, &[b"tail: string"]);

            let ended = lower("ended");
            let mir = ended
                .mir()
                .expect("discarded descriptor releases its backing");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let prefix = straight_blocks(mir, mir.entry);
            let [borrow, build, moved, test] = prefix.as_slice() else {
                panic!("borrow the actual local owner before staging its carrier");
            };
            let Operation::Borrow {
                source: backing,
                holder,
                exclusive: false,
            } = borrow.operation
            else {
                panic!("shared loan of the owned string");
            };
            assert_eq!(backing.root.span.bytes(), b"local: string");
            assert_eq!(borrow.site.span.bytes(), b"&local");
            let Operation::ConstructVariant {
                variant,
                payload: Some(payload),
                into: input,
            } = build.operation
            else {
                panic!("store the view in canonical Some");
            };
            assert_eq!(variant, &option.some.id);
            assert!(same(holder, payload));
            let Operation::Copy {
                from,
                into: Some(carrier),
            } = moved.operation
            else {
                panic!("stage an independent copy of the local carrier");
            };
            assert!(same(from, input));
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, .. }
                if same(operand, carrier))
            );
            let success = linear(mir, yes);
            assert!(matches!(success.as_slice(),
                [Operation::TakeOptionPayload { operand, into }, Operation::Cleanup(cleanup),
                 Operation::Dispose { operand: released, .. }, ..]
                if same(*operand, carrier) && same(*released, backing)
                    && matches!(cleanup.as_slice(), [Obligation::EndStorage(view)] if same(*view, *into))));
            assert_return(&success, &option.some.id, true, &[]);
            let failure = linear(mir, no);
            assert!(matches!(failure.as_slice(),
                [Operation::EndEmptyOption(operand), Operation::ConstructVariant { payload: None, .. },
                 Operation::Export(_), ..] if same(*operand, carrier)));
            assert!(
                !failure
                    .iter()
                    .any(|op| matches!(op, Operation::Dispose { .. }))
            );
            assert_return(&failure, &option.none.id, false, &[b"local: string"]);

            for (name, code, site, related) in [
                (
                    "escaped",
                    "R0202",
                    b"return Option.Some(refValue);".as_slice(),
                    b"local: string".as_slice(),
                ),
                (
                    "releaseLive",
                    "R0203",
                    b"dispose(local)".as_slice(),
                    b"&local".as_slice(),
                ),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("one finding for {name}");
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), related);
            }
            // Keep the shared routes at the bounds measured before exclusive forwarding.
            for (name, work, entries, depth) in
                [("forward", 3709, 607, 4), ("ended", 5240, 1037, 5)]
            {
                let exact = AnalysisLimits {
                    max_work: work,
                    max_entries: entries,
                    max_depth: depth,
                };
                assert_eq!(check(name, exact).completion(), Completion::Checked);
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
                    let limited = check(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

#[test]
fn exclusive_option_postfix_moves_the_carrier_and_descriptor() {
    use compiler::analysis::Stage;
    probe_with_typed(
        "propagation::exclusive_option_postfix_moves_the_carrier_and_descriptor",
        r#"
        function forward(input: Option<&mut bool>, tail: string): Option<&mut bool> {
            let refValue = ((input))?;
            return Option.Some(refValue);
        }
        "#,
        |request, module, receipt, typed| {
            assert_eq!(
                receipt.not_performed(),
                &[Stage::Ownership, Stage::Execution]
            );
            let option = typed.option_role().unwrap();
            let function = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("forward"))
                .unwrap();
            let check = |limits| lower_body(receipt, module, &function.id, limits).unwrap();
            let report = check(AnalysisLimits::default());
            assert_eq!(
                report.completion(),
                Completion::Checked,
                "test_mode={}; blocked at {:?}",
                request.test_mode,
                report.blocked_at().map(|s| s.span.bytes())
            );
            assert!(report.findings().is_empty());
            let mir = report.mir().unwrap();
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"((input))");
            let prefix = straight_blocks(mir, mir.entry);
            let [moved, test] = prefix.as_slice() else {
                panic!("stage once before testing")
            };
            let Operation::Move {
                from,
                into: Some(carrier),
            } = moved.operation
            else {
                panic!("move the actual carrier")
            };
            let same = |left: compiler::analysis::ownership::Place<'_>,
                        right: compiler::analysis::ownership::Place<'_>| {
                left.projection.is_none()
                    && right.projection.is_none()
                    && left.root.node == right.root.node
            };
            assert_eq!(from.root.span.bytes(), b"input: Option<&mut bool>");
            assert!(!same(from, carrier));
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, .. } if same(operand, carrier))
            );
            let success = linear(mir, yes);
            let [
                Operation::TakeOptionPayload {
                    operand,
                    into: binding,
                },
                Operation::Move {
                    from,
                    into: Some(descriptor),
                },
                Operation::ConstructVariant {
                    variant,
                    payload: Some(payload),
                    into,
                },
                Operation::Export(out),
                ..,
            ] = success.as_slice()
            else {
                panic!("take and move one descriptor into the exported Some")
            };
            assert!(same(*operand, carrier));
            assert_eq!(binding.root.span.bytes(), b"refValue");
            assert!(same(*from, *binding) && same(*descriptor, *payload) && same(*into, *out));
            assert_eq!(*variant, &option.some.id);
            assert_return(&success, &option.some.id, true, &[b"tail: string"]);
            let failure = linear(mir, no);
            assert!(matches!(failure.as_slice(),
                [Operation::EndEmptyOption(operand), Operation::ConstructVariant { payload: None, .. },
                 Operation::Export(_), ..] if same(*operand, carrier)));
            assert_return(&failure, &option.none.id, false, &[b"tail: string"]);
            for op in success.iter().chain(&failure) {
                if let Operation::Cleanup(cleanup) = op {
                    assert!(cleanup.iter().all(|entry| !matches!(entry,
                        Obligation::EndStorage(p) if same(*p, carrier) || same(*p, *binding)
                            || p.root.span.bytes() == b"input: Option<&mut bool>")));
                }
            }

            let stats = report.statistics();
            let exact = AnalysisLimits {
                max_work: stats.work,
                max_entries: stats.entries,
                max_depth: stats.max_depth,
            };
            assert_eq!(check(exact).completion(), Completion::Checked);
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
                let limited = check(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none() && limited.findings().is_empty());
            }
        },
    );
}

#[test]
fn known_some_postfix_moves_the_exclusive_carrier_without_a_tag_test() {
    probe_with_typed(
        "propagation::known_some_postfix_moves_the_exclusive_carrier_without_a_tag_test",
        r#"
            function present(input: Option<&mut bool>.Some): Option<&mut bool> {
                let view = input?; return Option.Some(view);
            }
            function reuse(input: Option<&mut bool>.Some): Option<&mut bool> {
                let _ = input?; return input;
            }
        "#,
        |_, module, receipt, typed| {
            let lower = |name: &str| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap()
            };
            let present = lower("present");
            let mir = present
                .mir()
                .expect("known Some must take its only feasible edge");
            let path = linear(mir, mir.entry);
            assert!(matches!(path.as_slice(), [
                Operation::Move { from: input, into: Some(stage) },
                Operation::TakeOptionPayload { operand, into: view },
                Operation::Move { from, into: Some(payload) },
                Operation::ConstructVariant { variant, payload: Some(value), into },
                Operation::Export(ret), ..
            ] if input.root.span.bytes() == b"input: Option<&mut bool>.Some"
                && stage.root.node == operand.root.node && view.root.node == from.root.node
                && payload.root.node == value.root.node && into.root.node == ret.root.node
                && *variant == &typed.option_role().unwrap().some.id));
            assert!(!mir.blocks.iter().any(|b| matches!(
                b.operation,
                Operation::TestVariant { .. } | Operation::EndEmptyOption(_)
            )));
            let reuse = lower("reuse");
            assert_eq!(reuse.completion(), Completion::SourceRejected);
            assert!(reuse.mir().is_none());
            let [finding] = reuse.findings() else {
                panic!("expected the reached second read")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"input");
            assert_eq!(finding.related.span.bytes(), b"input");
            assert!(finding.related.span.start() < finding.site.span.start());
        },
    );
}

#[test]
fn exclusive_option_postfix_checks_reached_moves_and_loans() {
    probe_with_typed(
        "propagation::exclusive_option_postfix_checks_reached_moves_and_loans",
        r#"
        function reuse(input: Option<&mut bool>): Option<&mut bool> {
            let _ = input?;
            return input;
        }
        function duplicate(input: Option<&mut bool>): Option<&mut bool> {
            let view = input?;
            let saved = Option.Some(view);
            return Option.Some(view);
        }
        function escape(): Option<&mut bool> {
            let mut flag = false;
            let input: Option<&mut bool> = Option.Some(&mut flag);
            let view = input?;
            return Option.Some(view);
        }
        function conflict(): Option<bool> {
            let mut flag = false;
            let input: Option<&mut bool> = Option.Some(&mut flag);
            let view = input?;
            let _ = &flag;
            return Option.Some(*view);
        }
        function release(): Option<bool> {
            let mut flag = false;
            let input: Option<&mut bool> = Option.Some(&mut flag);
            let view = input?;
            dispose(flag);
            return Option.Some(*view);
        }
        function dead(): Option<bool> {
            let mut flag = false;
            let input: Option<&mut bool> = Option.Some(&mut flag);
            let view = input?;
            let result = *view;
            dispose(flag);
            return Option.Some(result);
        }
        function empty(tail: string): Option<&mut bool> {
            let input = Option<&mut bool>.None;
            let _ = input?;
            return input;
        }
        "#,
        |_, module, receipt, typed| {
            let check = |name| {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &function.id, AnalysisLimits::default()).unwrap()
            };
            let at = |span: &'static [u8]| span;
            for (name, code, site, related) in [
                ("reuse", "R0200", at(b"input"), at(b"input")),
                ("duplicate", "R0200", at(b"view"), at(b"view")),
                (
                    "escape",
                    "R0202",
                    at(b"return Option.Some(view);"),
                    at(b"flag"),
                ),
                ("conflict", "R0201", at(b"&flag"), at(b"&mut flag")),
                ("release", "R0203", at(b"dispose(flag)"), at(b"&mut flag")),
            ] {
                let report = check(name);
                assert_eq!(
                    report.completion(),
                    Completion::SourceRejected,
                    "{name}: blocked at {:?}",
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("one finding for {name}")
                };
                assert_eq!(
                    (
                        finding.code(),
                        finding.site.span.bytes(),
                        finding.related.span.bytes()
                    ),
                    (code, site, related)
                );
                assert_eq!(finding.site.module, &module.key);
                assert_eq!(finding.related.module, &module.key);
                assert!(finding.site.span.start() > finding.related.span.start());
            }
            let dead = check("dead");
            assert_eq!(
                dead.completion(),
                Completion::Checked,
                "dead: blocked at {:?}",
                dead.blocked_at()
                    .map(|s| s.span.bytes().escape_ascii().to_string())
            );
            assert!(dead.findings().is_empty());
            let mir = dead.mir().unwrap();
            let prefix = straight_blocks(mir, mir.entry);
            let [_, borrow, build, moved, test] = prefix.as_slice() else {
                panic!("initialize the flag, borrow it, construct and move the carrier, then test");
            };
            let Operation::Borrow {
                source,
                holder,
                exclusive: true,
            } = borrow.operation
            else {
                panic!("exclusive backing of the payload");
            };
            assert_eq!(source.root.span.bytes(), b"flag");
            assert_eq!(borrow.site.span.bytes(), b"&mut flag");
            let Operation::ConstructVariant {
                payload: Some(payload),
                into: original,
                ..
            } = build.operation
            else {
                panic!("install the borrowed descriptor in the carrier");
            };
            assert_eq!(holder.root.node, payload.root.node);
            let Operation::Move {
                from,
                into: Some(stage),
            } = moved.operation
            else {
                panic!("move that carrier");
            };
            assert_eq!(from.root.node, original.root.node);
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, .. } if operand.root.node == stage.root.node)
            );
            let (yes, _) = branch_targets(mir);
            let success = linear(mir, yes);
            assert!(matches!(success.as_slice(),
                [Operation::TakeOptionPayload { operand, into }, Operation::Dereference(descriptor),
                 Operation::CallScalarDispose { operand: released, .. }, ..]
                if operand.root.node == stage.root.node && into.root.node == descriptor.root.node
                    && released.root.node == source.root.node));

            let empty = check("empty");
            assert_eq!(empty.completion(), Completion::Checked);
            assert!(empty.findings().is_empty());
            let mir = empty.mir().unwrap();
            let path = linear(mir, mir.entry);
            assert!(matches!(path.as_slice(),
                [Operation::ConstructVariant { payload: None, into: input, .. }, Operation::Copy { from, into: Some(stage) },
                 Operation::EndEmptyOption(ended), ..] if input.root.node == from.root.node && stage.root.node == ended.root.node));
            let original = match path[0] {
                Operation::ConstructVariant { into, .. } => into.root.node,
                _ => unreachable!(),
            };
            let export = path
                .iter()
                .position(|op| matches!(op, Operation::Export(_)))
                .unwrap();
            assert!(path[export+1..].iter().any(|op|matches!(op,Operation::Cleanup(items) if items.iter().any(|item|matches!(item,Obligation::EndStorage(p) if p.root.node==original)))));
            assert_return(
                &path,
                &typed.option_role().unwrap().none.id,
                false,
                &[b"tail: string"],
            );
        },
    );
}

#[test]
fn reborrowing_an_extracted_exclusive_descriptor_preserves_its_restriction() {
    probe(
        "propagation::reborrowing_an_extracted_exclusive_descriptor_preserves_its_restriction",
        r#"
        function suspended(input: Option<&mut bool>): Option<&mut bool> {
            let view = input?; let result = Option.Some(&mut *view);
            let bad = *view; return result;
        }
        function resumed(input: Option<&mut bool>): Option<bool> {
            let view = input?;
            { let child = &mut *view; let bit = *child; }
            return Option.Some(*view);
        }
        function escaped(): Option<&mut bool> {
            let mut flag = false; let owner: Option<&mut bool> = Option.Some(&mut flag);
            let view = owner?; return Option.Some(&mut *view);
        }
    "#,
        |_, module, receipt| {
            let run = |name: &str| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap()
            };
            for (name, code, site, related) in [
                (
                    "suspended",
                    "R0201",
                    b"*view".as_slice(),
                    b"&mut *view".as_slice(),
                ),
                (
                    "escaped",
                    "R0202",
                    b"return Option.Some(&mut *view);",
                    b"flag",
                ),
            ] {
                let report = run(name);
                assert_eq!(report.completion(), Completion::SourceRejected);
                assert!(report.mir().is_none());
                let [f] = report.findings() else {
                    panic!("one reached invalid access")
                };
                assert_eq!(
                    (f.code(), f.site.span.bytes(), f.related.span.bytes()),
                    (code, site, related)
                );
            }
            let resumed = run("resumed");
            let mir = resumed.mir().unwrap();
            let (yes, _) = branch_targets(mir);
            let path = linear(mir, yes);
            let at = path
                .iter()
                .position(|op| {
                    matches!(
                        op,
                        Operation::Reborrow {
                            exclusive: true,
                            ..
                        }
                    )
                })
                .unwrap();
            assert!(path[at + 1..].iter().any(
                |op| matches!(op,Operation::Dereference(view) if view.root.span.bytes()==b"view")
            ));
        },
    );
}

#[test]
fn exclusive_option_postfix_does_not_prove_call_or_slot_contracts() {
    probe(
        "propagation::exclusive_option_postfix_does_not_prove_call_or_slot_contracts",
        r#"
        function forward(input: Option<&mut bool>): Option<&mut bool> {
            let view = input?;
            return Option.Some(view);
        }
        function caller(input: Option<&mut bool>): Option<&mut bool> {
            return forward(input);
        }
        function callerPostfix(input: Option<&mut bool>): Option<&mut bool> {
            let view = forward(input)?;
            return Option.Some(view);
        }
        function otherPayload(input: Option<&mut i32>): Option<&mut i32> {
            let view = input?;
            return Option.Some(view);
        }
        "#,
        |_, module, receipt| {
            for (name, blocked) in [
                ("caller", b"forward(input)".as_slice()),
                ("callerPostfix", b"forward(input)?".as_slice()),
                ("otherPayload", b"input: Option<&mut i32>".as_slice()),
            ] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &function.id, AnalysisLimits::default()).unwrap();
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none() && report.findings().is_empty());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), blocked, "{name}");
            }
        },
    );
    probe_report(
        "propagation::exclusive_option_postfix_does_not_prove_call_or_slot_contracts",
        r#"
        function store(slot: &mut Option<&mut bool>, input: Option<&mut bool>): Option<bool> {
            let view = input?;
            *slot = Option.Some(view);
            return Option.Some(true);
        }
        "#,
        |_, module, typed| {
            use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none() && typed.bodies().findings().is_empty());
            let [pending] = typed.result_use().unwrap().pending() else {
                panic!("one unsupported slot")
            };
            assert_eq!(pending.blocker, ResultUseBlocker::Storage);
            assert_eq!(pending.site.module, &module.key);
            assert_eq!(pending.site.span.bytes(), b"*slot = Option.Some(view);");
        },
    );
}

#[test]
fn exclusive_option_does_not_copy_or_propagate_through_a_borrowed_whole() {
    use compiler::analysis::{
        scalars::{ScalarFindingKind, ScalarUnsupported},
        typed::TypedCompletion,
    };
    let test = "propagation::exclusive_option_does_not_copy_or_propagate_through_a_borrowed_whole";
    for borrow in ["&", "&mut "] {
        let source = format!(
            "function borrowed(input: {borrow}Option<&mut bool>): Option<bool> {{ let _ = input?; return Option.Some(true); }}"
        );
        probe_report(test, &source, |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            assert!(typed.bodies().findings().iter().any(|finding| {
                finding.kind == ScalarFindingKind::InvalidTarget
                    && finding.site.module == &module.key
                    && finding.site.span.bytes() == b"input?"
            }));
        });
    }
    probe_report(
        test,
        "function duplicate(input: Option<&mut bool>): Option<&mut bool> { return copy(&input); }",
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none() && typed.bodies().findings().is_empty());
            let [pending] = typed.bodies().obligations() else {
                panic!("no selected explicit copy")
            };
            assert_eq!(pending.reason, ScalarUnsupported::FamilySelection);
            assert_eq!(pending.site.module, &module.key);
            assert_eq!(pending.site.span.bytes(), b"copy(&input)");
        },
    );
}

#[test]
fn shared_option_copies_keep_the_named_carrier_and_its_backing() {
    probe(
        "propagation::shared_option_copies_keep_the_named_carrier_and_its_backing",
        r#"
        function make(view: &string): Option<&string> { return Option.Some(view); }
        function producer(view: &string): Option<&string> { return make(view); }
        function reuse(input: Option<&string>): Option<&string> {
            let view = input?; return input;
        }
        function grouped(input: Option<&string>): Option<&string> {
            let view = (((((((input)))))))?; return input;
        }
        function live(local: string): Option<bool> {
            let input: Option<&string> = Option.Some(&local);
            let _ = input?; dispose(local); let _ = input?; return Option.Some(true);
        }
        function beforeTake(local: string): Option<bool> {
            let input: Option<&string> = Option.Some(&local);
            let other = input; dispose(local); let _ = other?; return Option.Some(true);
        }
        function unused(local: string): Option<bool> {
            let input: Option<&string> = Option.Some(&local);
            dispose(local); return Option.Some(true);
        }
    "#,
        |_, module, receipt| {
            let run = |name: &str, limits| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            // The unsupported producer fit these exact limits before shared payload storage.
            let limit = AnalysisLimits {
                max_work: 2054,
                max_entries: 120,
                max_depth: 2,
            };
            let producer = run("producer", limit);
            assert_eq!(producer.completion(), Completion::Incomplete);
            assert!(producer.mir().is_none());
            for name in ["reuse", "grouped"] {
                let report = run(name, default);
                let mir = report.mir().expect("copy retains the named source");
                let (yes, _) = branch_targets(mir);
                assert!(matches!(
                    straight_blocks(mir, mir.entry)[0].operation,
                    Operation::Copy { .. }
                ));
                assert!(linear(mir,yes).iter().any(|op| matches!(op,Operation::Copy { from, into: Some(_) } if from.root.span.bytes()==b"input: Option<&string>")));
            }
            let limit = AnalysisLimits {
                max_depth: run("reuse", default).statistics().max_depth,
                ..default
            };
            let short = run("grouped", limit);
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
            for name in ["live", "beforeTake"] {
                let report = run(name, default);
                assert_eq!(report.completion(), Completion::SourceRejected);
                assert!(report.mir().is_none());
                let [f] = report.findings() else { panic!() };
                assert_eq!(
                    (f.code(), f.site.span.bytes(), f.related.span.bytes()),
                    ("R0203", b"dispose(local)".as_slice(), b"&local".as_slice())
                );
            }
            assert!(run("unused", default).mir().is_some());
        },
    );
}

#[test]
fn shared_option_postfix_uses_the_static_variant_edges() {
    probe(
        "propagation::shared_option_postfix_uses_the_static_variant_edges",
        r#"
        function widened(local: string): Option<bool> {
            let input: Option<&string> = Option<&string>.None;
            let _ = input?; dispose(local); dispose(local); return Option.Some(true);
        }
        function empty(local: string): Option<bool> {
            let input = Option<&string>.None;
            let _ = input?; dispose(local); dispose(local); return Option.Some(true);
        }
    "#,
        |_, module, receipt| {
            let check = |name: &str| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap()
            };
            let parent = check("widened");
            assert_eq!(parent.completion(), Completion::SourceRejected);
            assert!(parent.mir().is_none());
            let [finding] = parent.findings() else {
                panic!("one reached use after disposal")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(local)");
            assert_eq!(finding.related.span.bytes(), b"dispose(local)");
            assert!(finding.site.span.start() > finding.related.span.start());
            let known = check("empty");
            let mir = known
                .mir()
                .expect("no success edge for the statically known None");
            let path = linear(mir, mir.entry);
            assert!(
                path.iter()
                    .any(|op| matches!(op, Operation::EndEmptyOption(_)))
            );
            assert!(!path.iter().any(|op| matches!(
                op,
                Operation::TestVariant { .. }
                    | Operation::TakeOptionPayload { .. }
                    | Operation::Dispose { .. }
            )));
        },
    );
}

#[test]
fn borrowing_the_whole_shared_text_option_is_not_a_postfix_target() {
    use compiler::analysis::scalars::ScalarFindingKind;
    probe_report(
        "propagation::borrowing_the_whole_shared_text_option_is_not_a_postfix_target",
        r#"
            function borrowed(input: &Option<&string>): Option<&string> {
                let refValue = input?;
                return Option.Some(refValue);
            }
        "#,
        |_, module, typed| {
            assert!(typed.receipt().is_none());
            assert!(typed.bodies().findings().iter().any(|finding| {
                finding.kind == ScalarFindingKind::InvalidTarget
                    && finding.site.module == &module.key
                    && finding.site.span.bytes() == b"input?"
            }));
        },
    );
}

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
            // Bounds for the unchanged scalar route before exclusive payload support.
            let limits = AnalysisLimits {
                max_work: 9231,
                max_entries: 2151,
                max_depth: 5,
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
fn wrapping_rhs_keeps_ordinary_and_unadmitted_budgets() {
    probe(
        "propagation::wrapping_rhs_keeps_ordinary_and_unadmitted_budgets",
        r#"
        function marker(): Result<i32, i32> { return Err(7i32); }
        function number(value: i32): i32 { return value; }
        function wrapCall(input: Result<bool, i32>): Result<bool, i32> {
            let value = input ? (number(3i32)); return Result.Ok(value);
        }
        function postfix(input: Result<bool, i32>): Result<bool, i32> {
            let value = input?; return Result.Ok(value);
        }
        function nested(input: Result<bool, i32>): Result<bool, i32> {
            let value = input ? (number(marker()?)); return Result.Ok(value);
        }
        function step(value: i32): Result<i32, i32> { return Result.Ok(value); }
        function pipe(input: Result<bool, i32>): Result<bool, i32> {
            let value = input ? (7i32 |> step?); return Result.Ok(value);
        }
        function direct(input: Result<bool, i32>): Result<bool, i32> {
            let value = input ? (marker()?); return Result.Ok(value);
        }
        function grouped(input: Result<bool, i32>): Result<bool, i32> {
            let value = input ? (((((((((marker()?))))))))); return Result.Ok(value);
        }
    "#,
        |_, module, receipt| {
            let run = |name: &str, limit| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, limit).unwrap()
            };
            for (name, work, entries, depth, expected) in [
                ("wrapCall", 6089, 900, 6, Completion::Checked),
                ("postfix", 3690, 604, 4, Completion::Checked),
                ("nested", 3237, 352, 4, Completion::Incomplete),
                ("pipe", 3237, 352, 4, Completion::Incomplete),
            ] {
                let limit = AnalysisLimits {
                    max_work: work,
                    max_entries: entries,
                    max_depth: depth,
                };
                let report = run(name, limit);
                assert_eq!(report.completion(), expected, "{name}");
                assert_eq!(report.mir().is_some(), expected == Completion::Checked);
                for short in [
                    AnalysisLimits {
                        max_entries: entries - 1,
                        ..limit
                    },
                    AnalysisLimits {
                        max_work: work - 1,
                        ..limit
                    },
                ] {
                    let exhausted = run(name, short);
                    assert_eq!(
                        exhausted.completion(),
                        Completion::ResourceLimit,
                        "{name} one short"
                    );
                    assert!(exhausted.mir().is_none());
                }
            }
            let direct = run("direct", AnalysisLimits::default());
            assert!(direct.mir().is_some());
            assert!(run("grouped", AnalysisLimits::default()).mir().is_some());
            let short = run(
                "grouped",
                AnalysisLimits {
                    max_depth: direct.statistics().max_depth,
                    ..Default::default()
                },
            );
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
        },
    );
}

#[test]
fn returning_wrap_rhs_forwards_instead_of_wrapping() {
    use compiler::analysis::ownership::{Block, Place};
    use compiler::analysis::scalars::{NodeFactKind, data::DataExpression};
    probe_with_typed(
        "propagation::returning_wrap_rhs_forwards_instead_of_wrapping",
        r#"
            import std::result::Err as fail;
            function marker(): Result<i32, i32> {
                let seed = fail<i32, i32>(7i32);
                let value = seed ? 9i32;
                return Result.Ok(value);
            }
            function after(flag: bool): bool { return flag; }
            function wrapsAfterRhs(input: Result<bool, i32>, tail: string): Result<bool, i32> {
                let flag = input ? (marker()?);
                return Result.Ok(after(flag));
            }
            function pending(input: Result<bool, i32>.Err, tail: string): Result<bool, i32> {
                let _ = input ? (marker()?);
                return Result.Ok(after(true));
            }
            function carryFailure(source: Result<i32, i32>.Err): Result<i32, i32>.Err {
                return source;
            }
            function terminalRhs(input: Result<bool, i32>.Err, source: Result<i32, i32>.Err, tail: string): Result<bool, i32> {
                let _ = input ? (carryFailure(source)?);
                return Result.Ok(after(true));
            }
            function carrySuccess(): Result<i32, i32>.Ok { return Result.Ok(5i32); }
            function successfulRhs(input: Result<bool, i32>.Err, tail: string): Result<bool, i32> {
                let _ = input ? (carrySuccess()?);
                return Result.Ok(after(true));
            }
            function scalarWrap(input: Result<bool, i32>, tail: string): Result<bool, i32> {
                let flag = input ? 3i32;
                return Result.Ok(flag);
            }
            function mover(text: string): Result<i32, i32> { return fail(8i32); }
            function badMarker(text: string): Result<i32, i32> {
                dispose(text);
                dispose(text);
                return fail(9i32);
            }
            function reachesBadRhs(input: Result<bool, i32>.Err, text: string): Result<bool, i32> {
                let _ = input ? (badMarker(text)?);
                return Result.Ok(true);
            }
            function skipsBadRhs(input: Result<bool, i32>.Ok, text: string): Result<bool, i32> {
                let _ = input ? (badMarker(text)?);
                return Result.Ok(true);
            }
            function reachesMoved(input: Result<bool, i32>.Err, text: string): Result<bool, i32> {
                dispose(text);
                let _ = input ? (mover(text)?);
                return Result.Ok(true);
            }
            function skipsMoved(input: Result<bool, i32>.Ok, text: string): Result<bool, i32> {
                dispose(text);
                let _ = input ? (mover(text)?);
                return Result.Ok(true);
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
            let lower = |name| check(name, AnalysisLimits::default());
            let role = typed.result_role().unwrap();
            let pin = role.representation.unwrap().storage_source;
            let facts = |node| typed.bodies().node(module, node).unwrap().unwrap();
            let assert_wrap = |block: &Block<'_>,
                               left: Place<'_>,
                               right: Place<'_>,
                               export: &Block<'_>| {
                let Operation::WrapResultFailure {
                    operand,
                    payload,
                    into,
                    variant,
                    wrapper,
                    origin,
                    cause_display,
                } = block.operation
                else {
                    panic!("expected a new outer wrapper");
                };
                assert_eq!(operand.root.node, left.root.node);
                assert_eq!(payload.root.node, right.root.node);
                assert_eq!((variant, wrapper), (&role.err.id, &role.wrapper.id));
                assert!(
                    matches!(export.operation, Operation::Export(out) if out.root.node == into.root.node)
                );
                let NodeFactKind::Expression(value) = &facts(block.site.node).kind else {
                    panic!("expected typed infix expression");
                };
                let Some(DataExpression::Wrapping(data)) = &value.data else {
                    panic!("expected typed wrapping evidence");
                };
                assert_eq!(origin, &data.construction.origin);
                assert_eq!(origin.snapshot(), &module.snapshot);
                assert_eq!(origin.bytes(), b"?");
                assert_eq!(
                    origin.start(),
                    block.site.span.start() + operand.root.span.bytes().len() + 1
                );
                let retained = data.construction.retained.as_ref().unwrap();
                assert_eq!(retained.operand, operand.root.node);
                assert_eq!(data.construction.value, payload.root.node);
                let goal = typed.bodies().goal(&retained.display).unwrap();
                assert_eq!(
                    cause_display,
                    goal.selection.as_ref().unwrap().implementation
                );
            };

            // The operative marker return has its own origin and a real cause.
            let marker = lower("marker");
            let mir = marker.mir().expect("checked marker");
            let prefix = straight_blocks(mir, mir.entry);
            let [_, construct, stage, test] = prefix.as_slice() else {
                panic!("marker prefix");
            };
            let Operation::ConstructResultError {
                constructor,
                variant,
                wrapper,
                into: seed,
                ..
            } = construct.operation
            else {
                panic!("construct the marker's cause");
            };
            assert_eq!(constructor, &role.providers.unwrap().constructor.id);
            assert_eq!((variant, wrapper), (&role.err.id, &role.wrapper.id));
            let NodeFactKind::Expression(value) = &facts(construct.site.node).kind else {
                panic!();
            };
            let Some(DataExpression::ErrorConstruction(data)) = &value.data else {
                panic!();
            };
            assert!(data.retained.is_none());
            assert_eq!(construct.site.span.bytes(), b"fail<i32, i32>(7i32)");
            assert_eq!(data.origin.snapshot(), construct.site.span.snapshot());
            assert_eq!(data.origin.start(), construct.site.span.start());
            let Operation::Move {
                from,
                into: Some(held),
            } = stage.operation
            else {
                panic!();
            };
            assert_eq!(from.root.node, seed.root.node);
            let Terminator::Branch { no, .. } = test.terminator else {
                panic!();
            };
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, success }
                if operand.root.node == held.root.node && success == &role.ok.id)
            );
            let failure = straight_blocks(mir, no);
            let [init, wrap, export, ..] = failure.as_slice() else {
                panic!();
            };
            let Operation::Initialize(payload) = init.operation else {
                panic!();
            };
            assert_wrap(wrap, held, payload, export);
            assert!(matches!(
                failure.last().unwrap().terminator,
                Terminator::Return
            ));

            for name in ["wrapsAfterRhs", "pending", "terminalRhs", "successfulRhs"] {
                let report = lower(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let prefix = straight_blocks(mir, mir.entry);
                let Operation::Move {
                    from,
                    into: Some(left),
                } = prefix[0].operation
                else {
                    panic!("stage the left input once");
                };
                assert!(
                    from.root
                        .span
                        .bytes()
                        .starts_with(b"input: Result<bool, i32>")
                );
                assert_eq!(left.root.span.bytes(), b"input");
                let rhs = if name == "wrapsAfterRhs" {
                    assert_eq!(prefix.len(), 2);
                    let (ok, err) = variant_edges(mir, mir.entry, &role.ok.id, b"input");
                    assert_eq!(calls_on_path(mir, [true]), [id("after")]);
                    let skipped = linear(mir, ok);
                    assert!(
                        matches!(skipped[0], Operation::TakeResultSuccess { operand, into: Some(flag) }
                        if operand.root.node == left.root.node && flag.root.span.bytes() == b"flag")
                    );
                    assert_return(&skipped, &role.ok.id, true, &[b"tail: string"]);
                    assert!(!skipped.iter().any(|op| matches!(
                        op,
                        Operation::DropResultFailure { .. } | Operation::WrapResultFailure { .. }
                    )));
                    straight_blocks(mir, err)
                } else {
                    prefix[1..].to_vec()
                };
                let call = match name {
                    "terminalRhs" => {
                        assert!(
                            matches!(rhs[0].operation, Operation::Move { from, into: Some(stage) }
                            if from.root.span.bytes().starts_with(b"source:") && stage.root.span.bytes() == b"source")
                        );
                        assert!(
                            matches!(rhs[1].operation, Operation::Call(callee) if callee == id("carryFailure"))
                        );
                        rhs[1]
                    }
                    "successfulRhs" => {
                        assert!(
                            matches!(rhs[0].operation, Operation::Call(callee) if callee == id("carrySuccess"))
                        );
                        rhs[0]
                    }
                    _ => {
                        let [call, test] = rhs.as_slice() else {
                            panic!("one RHS call and test");
                        };
                        assert_eq!(call.site.span.bytes(), b"marker()");
                        assert!(
                            matches!(call.operation, Operation::Call(callee) if callee == id("marker"))
                        );
                        assert!(
                            matches!(test.operation, Operation::TestVariant { operand, success }
                            if operand.root.node == call.site.node && success == &role.ok.id)
                        );
                        call
                    }
                };
                let (ok, err) = match rhs.last().unwrap().terminator {
                    Terminator::Branch { yes, no }
                        if matches!(name, "wrapsAfterRhs" | "pending") =>
                    {
                        (
                            Some(straight_blocks(mir, yes)),
                            Some(straight_blocks(mir, no)),
                        )
                    }
                    Terminator::Return if name == "terminalRhs" => (None, Some(rhs[2..].to_vec())),
                    Terminator::Return if name == "successfulRhs" => {
                        (Some(rhs[1..].to_vec()), None)
                    }
                    _ => panic!("wrong RHS feasibility for {name}"),
                };
                if let Some(ok) = ok {
                    let [take, wrap, export, cleanup @ ..] = ok.as_slice() else {
                        panic!("wrap after RHS success");
                    };
                    let Operation::TakeResultSuccess {
                        operand,
                        into: Some(payload),
                    } = take.operation
                    else {
                        panic!();
                    };
                    assert_eq!(operand.root.node, call.site.node);
                    assert_wrap(wrap, left, payload, export);
                    assert_wrap_tail_cleanup(cleanup);
                }
                if let Some(err) = err {
                    let [forward, export, drop, cleanup @ ..] = err.as_slice() else {
                        panic!("forward, export and retire the left failure");
                    };
                    let Operation::ForwardResultFailure {
                        operand,
                        into,
                        variant,
                        wrapper,
                    } = forward.operation
                    else {
                        panic!();
                    };
                    assert_eq!(operand.root.node, call.site.node);
                    assert_eq!(into.root.node, forward.site.node);
                    assert_eq!((variant, wrapper), (&role.err.id, &role.wrapper.id));
                    assert_ne!(into.root.node, left.root.node);
                    assert!(
                        matches!(export.operation, Operation::Export(out) if out.root.node == into.root.node)
                    );
                    assert!(
                        matches!(drop.operation, Operation::DropResultFailure { operand, variant, wrapper, storage_source }
                        if operand.root.node == left.root.node && variant == &role.err.id
                            && wrapper == &role.wrapper.id && storage_source == pin)
                    );
                    assert_wrap_tail_cleanup(cleanup);
                }

                let stats = report.statistics();
                let limits = AnalysisLimits {
                    max_work: stats.work,
                    max_entries: stats.entries,
                    max_depth: stats.max_depth,
                };
                assert!(
                    check(name, limits).mir().is_some(),
                    "exact limits for {name}"
                );
                for short in [
                    AnalysisLimits {
                        max_work: limits.max_work - 1,
                        ..limits
                    },
                    AnalysisLimits {
                        max_entries: limits.max_entries - 1,
                        ..limits
                    },
                    AnalysisLimits {
                        max_depth: limits.max_depth - 1,
                        ..limits
                    },
                ] {
                    let limited = check(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none());
                }
            }

            // Measured in both modes on 66d553e, with this source installed.
            let ordinary = check(
                "scalarWrap",
                AnalysisLimits {
                    max_work: 4080,
                    max_entries: 801,
                    max_depth: 4,
                },
            );
            let mir = ordinary
                .mir()
                .expect("normal wrapping still fits its original budget");
            let (_, err) = variant_edges(mir, mir.entry, &role.ok.id, b"input");
            let path = straight_blocks(mir, err);
            let [init, wrap, export, cleanup @ ..] = path.as_slice() else {
                panic!("ordinary wrap path");
            };
            let Operation::Initialize(payload) = init.operation else {
                panic!();
            };
            let Operation::WrapResultFailure { operand, .. } = wrap.operation else {
                panic!();
            };
            assert_wrap(wrap, operand, payload, export);
            assert_wrap_tail_cleanup(cleanup);

            for (name, drops) in [
                ("skipsBadRhs", &[b"text: string".as_slice()][..]),
                ("skipsMoved", &[][..]),
            ] {
                let skipped = lower(name);
                let mir = skipped.mir().expect("left Ok must skip RHS entirely");
                assert!(calls_on_path(mir, []).is_empty());
                let path = linear(mir, mir.entry);
                assert_return(&path, &role.ok.id, true, drops);
                assert!(!path.iter().any(|op| matches!(
                    op,
                    Operation::DropResultFailure { .. }
                        | Operation::WrapResultFailure { .. }
                        | Operation::ForwardResultFailure { .. }
                )));
            }
            assert!(lower("mover").mir().is_some());
            let rejection = |name| {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("one finding for {name}");
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.related.span.bytes(), b"dispose(text)");
                assert!(finding.related.span.start() < finding.site.span.start());
                (finding.site, finding.related)
            };
            let (bad_use, bad_move) = rejection("badMarker");
            assert_eq!(bad_use.span.bytes(), b"dispose(text)");
            let (callee_use, callee_move) = rejection("reachesBadRhs");
            assert_eq!(
                (callee_use.node, callee_move.node),
                (bad_use.node, bad_move.node)
            );
            let (use_site, moved_site) = rejection("reachesMoved");
            assert_eq!(use_site.span.bytes(), b"text");
            assert_ne!(moved_site.node, bad_move.node);
            assert!(use_site.span.start() > id("reachesMoved").start);
            assert!(moved_site.span.start() > id("reachesMoved").start);
        },
    );
}

fn assert_wrap_tail_cleanup(path: &[&compiler::analysis::ownership::Block<'_>]) {
    let mut obligations = path.iter().flat_map(|block| match &block.operation {
        Operation::Cleanup(actions) => actions.as_slice(),
        Operation::None => &[],
        other => panic!("unexpected wrapping exit operation: {other:?}"),
    });
    assert!(
        matches!(obligations.next(), Some(Obligation::Drop(place)) if place.root.span.bytes() == b"tail: string")
    );
    assert!(obligations.next().is_none());
    assert!(matches!(
        path.last().unwrap().terminator,
        Terminator::Return
    ));
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
fn discarded_string_result_is_distinct_from_the_staged_option_payload() {
    probe_with_typed(
        "propagation::discarded_string_result_is_distinct_from_the_staged_option_payload",
        r#"
            function text(value: string, bit: bool): string { return value; }
            function after(owner: string): bool { dispose(owner); return true; }
            function complete(input: Option<string>, gate: Option<bool>, keep: string): Option<bool> {
                text(input?, gate?);
                let bit = after(keep);
                return Option.Some(bit);
            }
            function reused(input: Option<string>, gate: Option<bool>, keep: string): Option<bool> {
                text(input?, gate?);
                let _ = input?;
                let bit = after(keep);
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
            let lower =
                |name| lower_body(receipt, module, id(name), AnalysisLimits::default()).unwrap();
            let option = typed.option_role().unwrap();
            let complete = lower("complete");
            let mir = complete
                .mir()
                .unwrap_or_else(|| panic!("complete: {:?}", complete.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (both, gate_no) = variant_edges(mir, yes, &option.some.id, b"gate");
            let entry: Vec<_> = straight_blocks(mir, mir.entry)
                .into_iter()
                .map(|block| &block.operation)
                .filter(|op| matches!(op, Operation::Move { .. } | Operation::TestVariant { .. }))
                .collect();
            let [
                Operation::Move {
                    from,
                    into: Some(carrier),
                },
                Operation::TestVariant { operand, .. },
            ] = entry.as_slice()
            else {
                panic!("expected one carrier move before testing the variant")
            };
            assert_eq!(from.root.span.bytes(), b"input: Option<string>");
            assert_eq!(carrier.root.node, operand.root.node);
            assert!(
                [from, carrier, operand]
                    .iter()
                    .all(|place| place.projection.is_none())
            );
            let staged = straight_blocks(mir, yes)
                .into_iter()
                .find_map(|block| match &block.operation {
                    Operation::TakeOptionPayload { operand, into }
                        if operand.root.node == carrier.root.node
                            && operand.projection.is_none()
                            && into.root.span.bytes() == b"input?" =>
                    {
                        Some(into.root.node)
                    }
                    _ => None,
                })
                .expect("stage the owned payload before testing gate");
            assert!(calls_on_path(mir, [false]).is_empty());
            assert!(calls_on_path(mir, [true, false]).is_empty());
            assert_return(&linear(mir, no), &option.none.id, false, &[b"keep: string"]);
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[b"input?", b"keep: string"],
            );
            assert!(matches!(path_drops(mir, gate_no).first(),
                Some(Obligation::Drop(place)) if place.root.node == staged));

            assert_eq!(calls_on_path(mir, [true, true]), [id("text"), id("after")]);
            let success: Vec<_> = straight_blocks(mir, both)
                .into_iter()
                .filter(|block| match &block.operation {
                    Operation::Call(_) | Operation::Drop(_) => true,
                    Operation::Move { from, .. } => from.root.span.bytes() == b"keep: string",
                    Operation::Cleanup(actions) => {
                        actions.iter().any(|op| matches!(op, Obligation::Drop(_)))
                    }
                    _ => false,
                })
                .collect();
            let [call, cleanup, stage_keep, after] = success.as_slice() else {
                panic!("expected call, discard cleanup, and continuation: {success:?}");
            };
            assert!(matches!(&call.operation, Operation::Call(callee) if *callee == id("text")));
            assert_eq!(call.site.span.bytes(), b"text(input?, gate?)");
            assert!(matches!(&cleanup.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::Drop(place)]
                    if place.root.node == call.site.node && place.projection.is_none())));
            assert!(
                matches!(&stage_keep.operation, Operation::Move { into: Some(into), .. }
                if into.root.span.bytes() == b"keep")
            );
            assert!(matches!(&after.operation, Operation::Call(callee) if *callee == id("after")));
            let Terminator::Goto(suffix) = after.terminator else {
                panic!("the continuation call must reach the return");
            };
            assert_return(&linear(mir, suffix), &option.some.id, true, &[]);

            let reused = lower("reused");
            assert_eq!(reused.completion(), Completion::SourceRejected);
            assert!(reused.mir().is_none());
            let [finding] = reused.findings() else {
                panic!("expected one moved-carrier finding");
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"input");
            assert_eq!(finding.related.span.bytes(), b"input");
            assert!(finding.related.span.start() < finding.site.span.start());
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
            // Bounds for the unchanged owning-text route before exclusive payload support.
            let named = lower(
                "named",
                AnalysisLimits {
                    max_work: 7461,
                    max_entries: 1249,
                    max_depth: 5,
                },
            );
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
            function skippedComposition(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
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
            let skipped = lower("skippedComposition", default);
            let mir = skipped
                .mir()
                .expect("skip the written composition and invalid outer step");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[b"stage(before)", b"input: Option<string>"],
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
            function deepDirect(before: string, input: Option<string>): Option<bool> {
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
            function skipPlainCarrier(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = laterString(stage(before), gate?, (stage(before) |> bounce?));
                return Option.Some(true);
            }
            function skipGrouped(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = laterString(stage(before), gate?, (((((((input?)))))) |> bounce?));
                return Option.Some(true);
            }
            function skipDeep(before: string, gate: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = laterString(stage(before), gate?, (((input?) |> bounce?) |> bounce?));
                return Option.Some(true);
            }
            function pick(bit: i32, input: string): Option<string> { return Option.Some(input); }
            function skipBadCarrier(before: string, gate: Option<bool>.None): Option<bool> {
                let _ = laterString(stage(before), gate?, (stage(before) |> pick(Option<i32>.Some(1i32)?, _)?));
                return Option.Some(true);
            }
            function skipInvalid(before: string, first: Option<bool>.None, input: Option<string>, inside: string, gate: Option<bool>, bit: bool): Option<bool> {
                let _ = laterString(stage(before), first?, ((input?) |> invalidInner(if (bit) stage(inside) else stop(), gate?, _)?));
                return Option.Some(true);
            }
            function skipNestedOperand(before: string, first: Option<bool>.None, input: Option<string>, inside: string, gate: Option<bool>): Option<bool> {
                let _ = laterString(stage(before), first?, ((input?) |> finish(stage(inside), both(gate?, true), _)?));
                return Option.Some(true);
            }
            function choose(bit: bool, carrier: Option<bool>): Option<bool> { return carrier; }
            function skipNonPrimitive(first: Option<bool>.None, second: Option<bool>, carried: Option<bool>): Option<bool> {
                let _ = both(first?, ((second?) |> choose(carried)?));
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
            let skipped = lower("skipped", default);
            let mir = skipped
                .mir()
                .unwrap_or_else(|| panic!("skipped composition: {:?}", skipped.completion()));
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[b"stage(before)", b"input: Option<string>"],
            );
            let plain = lower("skipPlainCarrier", default);
            assert_eq!(mir.blocks.len(), plain.mir().unwrap().blocks.len());
            assert_eq!(skipped.statistics().entries, plain.statistics().entries);
            let invalid = lower("skipInvalid", default);
            let mir = invalid
                .mir()
                .expect("the skipped inner callee is not ownership checked");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[
                    b"stage(before)",
                    b"inside: string",
                    b"input: Option<string>",
                ],
            );
            let grouped = lower("skipGrouped", default);
            assert!(grouped.mir().is_some());
            assert!(grouped.statistics().max_depth > skipped.statistics().max_depth);
            let cap = AnalysisLimits {
                max_depth: skipped.statistics().max_depth,
                ..default
            };
            assert!(lower("skipped", cap).mir().is_some());
            let grouped = lower("skipGrouped", cap);
            assert_eq!(grouped.completion(), Completion::ResourceLimit);
            assert!(grouped.mir().is_none());
            let direct = lower("deepDirect", default);
            let mir = direct
                .mir()
                .expect("a direct call can stage a deeper composition");
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("stage"), id("bounce")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, false]),
                [id("stage"), id("bounce"), id("bounce")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true]),
                [id("stage"), id("bounce"), id("bounce"), id("useAll")]
            );
            let deep_skip = lower("skipDeep", default);
            let mir = deep_skip
                .mir()
                .expect("skip the deeper pipe without staging its input");
            assert_eq!(mir.blocks.len(), skipped.mir().unwrap().blocks.len());
            assert_eq!(deep_skip.statistics().entries, skipped.statistics().entries);
            assert!(deep_skip.statistics().work > skipped.statistics().work);
            assert!(deep_skip.statistics().max_depth > skipped.statistics().max_depth);
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[b"stage(before)", b"input: Option<string>"],
            );
            for name in ["skipBadCarrier", "skipNonPrimitive", "skipNestedOperand"] {
                let report = lower(name, default);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none(), "{name}");
            }
            assert_eq!(
                lower("skipBadCarrier", default)
                    .blocked_at()
                    .unwrap()
                    .span
                    .bytes(),
                b"Option<i32>.Some(1i32)?"
            );
            assert_eq!(
                lower("skipNonPrimitive", default)
                    .blocked_at()
                    .unwrap()
                    .span
                    .bytes(),
                b"(second?) |> choose(carried)?"
            );
            for (name, report) in [("nested", nested), ("skipInvalid", invalid)] {
                let exact = AnalysisLimits {
                    max_work: report.statistics().work,
                    max_entries: report.statistics().entries,
                    max_depth: report.statistics().max_depth,
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
                    AnalysisLimits {
                        max_depth: exact.max_depth - 1,
                        ..exact
                    },
                ] {
                    let limited = lower(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none());
                }
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
            let mir = skipped
                .mir()
                .expect("the wholly skipped sibling must remain lexical");
            assert!(calls_on_path(mir, []).is_empty());
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[b"right: Option<string>"],
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
            function invalid(value: string, bit: bool, prefix: string): bool {
                dispose(prefix); dispose(prefix); dispose(value); return bit;
            }
            function terminal(outer: string, input: Option<string>.None, inside: string, gate: Option<bool>, tail: string): Option<bool> {
                let _ = stage(outer) |> invalid(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _);
                return Option.Some(true);
            }
            function reached(outer: string, input: Option<string>.Some, inside: string, gate: Option<bool>.Some, tail: string): Option<bool> {
                let bit = stage(outer) |> invalid(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _);
                return Option.Some(bit);
            }
            function siblings(input: Option<string>, written: Option<string>, inside: string, gate: Option<bool>): Option<bool> {
                let bit = ((input?) |> present?) |> useAll(((written?) |> finish(stage(inside), gate?, _)?), true, _);
                return Option.Some(bit);
            }
            function skippedSibling(input: Option<string>.None, written: Option<string>): Option<bool> {
                let _ = ((input?) |> present?) |> useAll(((written?) |> present?), true, _);
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
            let reached = lower("reached", default);
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("expected one reached outer-step finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(prefix)");
            assert_eq!(finding.related.span.bytes(), b"dispose(prefix)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let siblings = lower("siblings", default);
            let mir = siblings
                .mir()
                .expect("composition of the input preserves the prefix for a written composition");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (written_yes, written_no) = variant_edges(mir, yes, &option.some.id, b"written");
            let (gate_yes, gate_no) = variant_edges(mir, written_yes, &option.some.id, b"gate");
            let (step_yes, step_no) = variant_edges(
                mir,
                gate_yes,
                &option.some.id,
                b"(written?) |> finish(stage(inside), gate?, _)?",
            );
            let input_stage = b"((input?) |> present?)";
            assert!(calls_on_path(mir, [false]).is_empty());
            assert!(
                matches!(path_drops(mir, no).as_slice(), [Obligation::Drop(inside), Obligation::DropOptionPayload { operand, some }]
                if inside.root.span.bytes() == b"inside: string" && operand.root.span.bytes() == b"written: Option<string>" && *some == &option.some.id)
            );
            assert_return(
                &linear(mir, written_no),
                &option.none.id,
                false,
                &[input_stage, b"inside: string"],
            );
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[b"stage(inside)", b"(written?)", input_stage],
            );
            assert_return(
                &linear(mir, step_no),
                &option.none.id,
                false,
                &[input_stage],
            );
            assert_return(&linear(mir, step_yes), &option.some.id, true, &[]);
            assert_eq!(
                calls_on_path(mir, [true, true, true, true]),
                [id("present"), id("stage"), id("finish"), id("useAll")]
            );
            let skipped = lower("skippedSibling", default);
            let mir = skipped
                .mir()
                .expect("skip the wholly unexecuted written composition");
            assert!(calls_on_path(mir, []).is_empty());
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[b"written: Option<string>"],
            );
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

#[test]
fn composed_arguments_before_a_propagating_outer_step_keep_all_exit_prefixes() {
    probe_with_typed(
        "propagation::composed_arguments_before_a_propagating_outer_step_keep_all_exit_prefixes",
        r#"
            function stage(value: string): string { return value; }
            function finish(inside: string, bit: bool, value: string): Option<string> {
                dispose(inside);
                if (bit) { return Option<string>.Some(value); }
                return Option<string>.None;
            }
            function outer(value: string, bit: bool, prefix: string): Option<string> {
                dispose(prefix);
                if (bit) { return Option<string>.Some(value); }
                return Option<string>.None;
            }
            function present(value: string): Option<string>.Some { return Option<string>.Some(value); }
            function last(tail: string): bool { dispose(tail); return true; }
            function after(bit: bool): bool { return bit; }
            function written(prefix: string, input: Option<string>, inside: string, gate: Option<bool>, tail: string): Option<bool> {
                let output = stage(prefix) |> outer(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _)?;
                return Option.Some(after(true));
            }
            function inputPosition(input: Option<string>, between: string, later: Option<bool>): Option<bool> {
                ((input?) |> present?) |> outer(stage(between), later?, _)?;
                return Option.Some(after(true));
            }
            function terminal(prefix: string, input: Option<string>.None, inside: string, gate: Option<bool>, tail: string): Option<bool> {
                let _ = stage(prefix) |> outer(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _)?;
                return Option.Some(true);
            }
            function plain(value: string, bit: bool, prefix: string): bool { dispose(prefix); dispose(value); return bit; }
            function terminalPlain(prefix: string, input: Option<string>.None, inside: string, gate: Option<bool>, tail: string): Option<bool> {
                let _ = stage(prefix) |> plain(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _);
                return Option.Some(true);
            }
            function invalid(value: string, bit: bool, prefix: string): Option<string>.None {
                dispose(prefix); dispose(prefix); dispose(value); return Option<string>.None;
            }
            function reached(prefix: string, input: Option<string>.Some): Option<bool> {
                let _ = stage(prefix) |> invalid(((input?) |> present?), true, _)?;
                return Option.Some(true);
            }
            function boolStep(value: string, bit: bool, prefix: string): Option<bool>.Some {
                dispose(value); dispose(prefix); return Option<bool>.Some(bit);
            }
            function scalarStep(prefix: string, input: Option<string>): Option<bool> {
                let bit = stage(prefix) |> boolStep(((input?) |> present?), true, _)?;
                return Option.Some(bit);
            }
            function borrowedStep(value: string, bit: &bool, prefix: string): Option<bool>.Some {
                dispose(value); dispose(prefix); return Option<bool>.Some(*bit);
            }
            function borrowed(prefix: string, input: Option<string>, bit: &bool): Option<bool> {
                let bit = stage(prefix) |> borrowedStep(((input?) |> present?), bit, _)?;
                return Option.Some(bit);
            }
            function siblings(input: Option<string>, written: Option<string>, inside: string, gate: Option<bool>): Option<bool> {
                let output = ((input?) |> present?) |> outer(((written?) |> finish(stage(inside), gate?, _)?), true, _)?;
                return Option.Some(true);
            }
            function skippedSibling(input: Option<string>.None, written: Option<string>): Option<bool> {
                let _ = ((input?) |> present?) |> outer(((written?) |> present?), true, _)?;
                return Option.Some(true);
            }
            function deepOuter(prefix: string, input: Option<string>): Option<bool> {
                let _ = stage(prefix) |> outer((((input?) |> present?) |> present?), true, _)?;
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
            let written = lower("written", default);
            let mir = written
                .mir()
                .unwrap_or_else(|| panic!("written before outer step: {:?}", written.completion()));
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (gate_yes, gate_no) = variant_edges(mir, yes, &option.some.id, b"gate");
            let (inner_yes, inner_no) = variant_edges(
                mir,
                gate_yes,
                &option.some.id,
                b"(input?) |> finish(stage(inside), gate?, _)?",
            );
            let (outer_yes, outer_no) = variant_edges(mir, inner_yes, &option.some.id, b"stage(prefix) |> outer(((input?) |> finish(stage(inside), gate?, _)?), last(tail), _)?");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(prefix)", b"tail: string", b"inside: string"],
            );
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[
                    b"stage(inside)",
                    b"(input?)",
                    b"stage(prefix)",
                    b"tail: string",
                ],
            );
            assert_return(
                &linear(mir, inner_no),
                &option.none.id,
                false,
                &[b"stage(prefix)", b"tail: string"],
            );
            assert_return(&linear(mir, outer_no), &option.none.id, false, &[]);
            assert_return(&linear(mir, outer_yes), &option.some.id, true, &[b"output"]);
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
                calls_on_path(mir, [true, true, true, false]),
                [
                    id("stage"),
                    id("stage"),
                    id("finish"),
                    id("last"),
                    id("outer")
                ]
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true, true]),
                [
                    id("stage"),
                    id("stage"),
                    id("finish"),
                    id("last"),
                    id("outer"),
                    id("after")
                ]
            );
            let input = lower("inputPosition", default);
            let mir = input
                .mir()
                .expect("composed input still evaluates before written arguments");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (later_yes, later_no) = variant_edges(mir, yes, &option.some.id, b"later");
            let (outer_yes, outer_no) = variant_edges(
                mir,
                later_yes,
                &option.some.id,
                b"((input?) |> present?) |> outer(stage(between), later?, _)?",
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
            assert_return(&linear(mir, outer_no), &option.none.id, false, &[]);
            let success: Vec<_> = linear(mir, outer_yes)
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
                matches!(success.as_slice(), [Operation::TakeOptionPayload { into, .. }, Operation::Cleanup(actions), Operation::Call(_)]
                if into.root.span.bytes() == b"((input?) |> present?) |> outer(stage(between), later?, _)?"
                    && matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.node == into.root.node)),
                "{success:?}"
            );
            assert_eq!(
                calls_on_path(mir, [true, true, true]),
                [id("present"), id("stage"), id("outer"), id("after")]
            );
            let terminal = lower("terminal", default);
            let mir = terminal
                .mir()
                .expect("known inner failure never reaches the outer step");
            assert_eq!(calls_on_path(mir, []), [id("stage")]);
            assert_return(
                &linear(mir, mir.entry),
                &option.none.id,
                false,
                &[b"stage(prefix)", b"tail: string", b"inside: string"],
            );
            let plain = lower("terminalPlain", default);
            assert!(plain.mir().is_some());
            assert_eq!(terminal.statistics().entries, plain.statistics().entries);
            let reached = lower("reached", default);
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("expected reached outer step failure")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(prefix)");
            assert_eq!(finding.related.span.bytes(), b"dispose(prefix)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let scalar = lower("scalarStep", default);
            let mir = scalar
                .mir()
                .expect("a checked primitive carrier admits the same argument layer");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[b"stage(prefix)"],
            );
            assert_return(&linear(mir, yes), &option.some.id, true, &[]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("present"), id("boolStep")]
            );
            let borrowed = lower("borrowed", default);
            assert_eq!(borrowed.completion(), Completion::Incomplete);
            assert!(borrowed.mir().is_none());
            assert_eq!(
                borrowed.blocked_at().unwrap().span.bytes(),
                b"stage(prefix) |> borrowedStep(((input?) |> present?), bit, _)?"
            );
            let siblings = lower("siblings", default);
            let mir = siblings
                .mir()
                .expect("both siblings can stage before a propagating step");
            let (yes, _) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (written_yes, written_no) = variant_edges(mir, yes, &option.some.id, b"written");
            let (gate_yes, gate_no) = variant_edges(mir, written_yes, &option.some.id, b"gate");
            let (inner_yes, inner_no) = variant_edges(
                mir,
                gate_yes,
                &option.some.id,
                b"(written?) |> finish(stage(inside), gate?, _)?",
            );
            let (step_yes, step_no) = variant_edges(mir, inner_yes, &option.some.id, b"((input?) |> present?) |> outer(((written?) |> finish(stage(inside), gate?, _)?), true, _)?");
            let staged = b"((input?) |> present?)";
            assert_return(
                &linear(mir, written_no),
                &option.none.id,
                false,
                &[staged, b"inside: string"],
            );
            assert_return(
                &linear(mir, gate_no),
                &option.none.id,
                false,
                &[b"stage(inside)", b"(written?)", staged],
            );
            assert_return(&linear(mir, inner_no), &option.none.id, false, &[staged]);
            assert_return(&linear(mir, step_no), &option.none.id, false, &[]);
            assert_return(&linear(mir, step_yes), &option.some.id, true, &[b"output"]);
            assert_eq!(
                calls_on_path(mir, [true, true, true, true, true]),
                [id("present"), id("stage"), id("finish"), id("outer")]
            );
            let skipped = lower("skippedSibling", default);
            let mir = skipped
                .mir()
                .expect("skip the written composition and the outer carrier");
            assert!(calls_on_path(mir, []).is_empty());
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[b"written: Option<string>"],
            );
            let deeper = lower("deepOuter", default);
            let mir = deeper
                .mir()
                .expect("both known-Some steps stage once before the outer step");
            assert_eq!(calls_on_path(mir, [false]), [id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("stage"), id("present"), id("present"), id("outer")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("stage"), id("present"), id("present"), id("outer")]
            );
            let exact = AnalysisLimits {
                max_work: written.statistics().work,
                max_entries: written.statistics().entries,
                max_depth: written.statistics().max_depth,
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
            let too_shallow = lower(
                "written",
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
            );
            assert_eq!(too_shallow.completion(), Completion::ResourceLimit);
            assert!(too_shallow.mir().is_none());
        },
    );
}

#[test]
fn recursive_pipe_operands_have_separate_failure_prefixes() {
    probe_with_typed(
        "propagation::recursive_pipe_operands_have_separate_failure_prefixes",
        r#"
            function stage(value: string): string { return value; }
            function step(value: string, bit: bool, owner: string): Option<string> {
                dispose(owner);
                if (bit) { return Option<string>.Some(value); }
                return Option<string>.None;
            }
            function present(value: string): Option<string>.Some { return Option<string>.Some(value); }
            function useText(bit: bool, value: string): bool { dispose(value); return bit; }
            function deep(prefix: string, middle: string, input: Option<string>, inside: string, first: Option<bool>, second: Option<bool>, third: Option<bool>, keep: string): Option<bool> {
                let output = stage(prefix) |> step((stage(middle) |> step(((input?) |> step(stage(inside), first?, _)?), second?, _)?), third?, _)?;
                return Option.Some(true);
            }
            function terminal(prefix: string, middle: string, input: Option<string>, inside: string, first: Option<bool>.None, second: Option<bool>, third: Option<bool>, keep: string): Option<bool> {
                let _ = stage(prefix) |> step((stage(middle) |> step(((input?) |> step(stage(inside), first?, _)?), second?, _)?), third?, _)?;
                return Option.Some(true);
            }
            function skipped(flag: Option<bool>.None, input: Option<string>): Option<bool> {
                let _ = useText(flag?, ((((input?) |> present?) |> present?) |> present?));
                return Option.Some(true);
            }
            function fromNumber(number: i32): Option<string> { return Option<string>.Some("value"); }
            function skippedBadCarrier(flag: Option<bool>.None): Option<bool> {
                let _ = useText(flag?, ((((Option<i32>.Some(7i32)?) |> fromNumber?) |> present?) |> present?));
                return Option.Some(true);
            }
            function bad(message: string): Option<string>.Some {
                dispose(message); dispose(message); return Option<string>.Some("value");
            }
            function reachedBad(seed: Option<string>.Some): Option<bool> {
                let _ = useText(true, ((((seed?) |> bad?) |> present?) |> present?));
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
            let deep = lower("deep", default);
            let mir = deep
                .mir()
                .unwrap_or_else(|| panic!("deep composition: {:?}", deep.completion()));
            let inner = b"(input?) |> step(stage(inside), first?, _)?";
            let middle = b"stage(middle) |> step(((input?) |> step(stage(inside), first?, _)?), second?, _)?";
            let outer = b"stage(prefix) |> step((stage(middle) |> step(((input?) |> step(stage(inside), first?, _)?), second?, _)?), third?, _)?";
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let (first_yes, first_no) = variant_edges(mir, yes, &option.some.id, b"first");
            let (inner_yes, inner_no) = variant_edges(mir, first_yes, &option.some.id, inner);
            let (second_yes, second_no) = variant_edges(mir, inner_yes, &option.some.id, b"second");
            let (middle_yes, middle_no) = variant_edges(mir, second_yes, &option.some.id, middle);
            let (third_yes, third_no) = variant_edges(mir, middle_yes, &option.some.id, b"third");
            let (outer_yes, outer_no) = variant_edges(mir, third_yes, &option.some.id, outer);
            for (failure, drops) in [
                (no, &[b"stage(middle)".as_slice(), b"stage(prefix)", b"keep: string", b"inside: string"][..]),
                (first_no, &[b"stage(inside)".as_slice(), b"(input?)", b"stage(middle)", b"stage(prefix)", b"keep: string"][..]),
                (inner_no, &[b"stage(middle)".as_slice(), b"stage(prefix)", b"keep: string"][..]),
                (second_no, &[b"((input?) |> step(stage(inside), first?, _)?)".as_slice(), b"stage(middle)", b"stage(prefix)", b"keep: string"][..]),
                (middle_no, &[b"stage(prefix)".as_slice(), b"keep: string"][..]),
                (third_no, &[b"(stage(middle) |> step(((input?) |> step(stage(inside), first?, _)?), second?, _)?)".as_slice(), b"stage(prefix)", b"keep: string"][..]),
                (outer_no, &[b"keep: string".as_slice()][..]),
            ] {
                assert_return(&linear(mir, failure), &option.none.id, false, drops);
            }
            assert_return(
                &linear(mir, outer_yes),
                &option.some.id,
                true,
                &[b"output", b"keep: string"],
            );
            assert_eq!(calls_on_path(mir, [false]), [id("stage"), id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true, true, true, true, true, true, true]),
                [
                    id("stage"),
                    id("stage"),
                    id("stage"),
                    id("step"),
                    id("step"),
                    id("step")
                ]
            );
            let terminal = lower("terminal", default);
            let mir = terminal
                .mir()
                .expect("a terminal deepest argument skips all three steps");
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            assert_eq!(calls_on_path(mir, [false]), [id("stage"), id("stage")]);
            assert_eq!(
                calls_on_path(mir, [true]),
                [id("stage"), id("stage"), id("stage")]
            );
            assert_return(
                &linear(mir, no),
                &option.none.id,
                false,
                &[
                    b"stage(middle)",
                    b"stage(prefix)",
                    b"keep: string",
                    b"inside: string",
                ],
            );
            assert_return(
                &linear(mir, yes),
                &option.none.id,
                false,
                &[
                    b"stage(inside)",
                    b"(input?)",
                    b"stage(middle)",
                    b"stage(prefix)",
                    b"keep: string",
                ],
            );
            let skipped = lower("skipped", default);
            let mir = skipped
                .mir()
                .expect("wholly skipped recursive composition is proof-only");
            assert!(calls_on_path(mir, []).is_empty());
            assert_option_cleanup_return(
                &linear(mir, mir.entry),
                option,
                &[b"input: Option<string>"],
            );
            let bad_carrier = lower("skippedBadCarrier", default);
            assert_eq!(bad_carrier.completion(), Completion::Incomplete);
            assert!(bad_carrier.mir().is_none());
            assert_eq!(
                bad_carrier.blocked_at().unwrap().span.bytes(),
                b"Option<i32>.Some(7i32)?"
            );
            let bad_call = lower("reachedBad", default);
            assert_eq!(bad_call.completion(), Completion::SourceRejected);
            assert!(bad_call.mir().is_none());
            let [finding] = bad_call.findings() else {
                panic!("expected the deepest reached callee finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(message)");
            assert_eq!(finding.related.span.bytes(), b"dispose(message)");
            assert!(finding.related.span.start() < finding.site.span.start());
            for (name, report) in [("deep", deep), ("skipped", skipped)] {
                let limits = AnalysisLimits {
                    max_work: report.statistics().work,
                    max_entries: report.statistics().entries,
                    max_depth: report.statistics().max_depth,
                };
                assert!(lower(name, limits).mir().is_some());
                for short in [
                    AnalysisLimits {
                        max_work: limits.max_work - 1,
                        ..limits
                    },
                    AnalysisLimits {
                        max_entries: limits.max_entries - 1,
                        ..limits
                    },
                    AnalysisLimits {
                        max_depth: limits.max_depth - 1,
                        ..limits
                    },
                ] {
                    let limited = lower(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none());
                }
            }
        },
    );
}

#[test]
fn skipped_compositions_still_check_result_carrier_producers() {
    probe_with_typed(
        "propagation::skipped_compositions_still_check_result_carrier_producers",
        r#"
            import std::result::{Err as fail, Ok as successValue};
            function both(left: bool, right: bool): bool { return right; }
            function relay(value: bool): Result<bool, i32> { return Result.Ok(value); }
            function cause(): fail<i32> { return cause(); }
            function known(value: bool): Result<bool, i32>.Ok { return Result<bool, i32>.Ok(value); }
            function deep(input: Result<bool, i32>, keep: string): Result<bool, i32> {
                let bit = ((((input?) |> known?) |> relay?) |> known?);
                return Result.Ok(bit);
            }
            function canonical(gate: Result<bool, i32>.Err): Result<bool, i32> {
                let _ = both(gate?, (((Result<bool, i32>.Ok(true)?) |> relay?) |> known?));
                return Result.Ok(true);
            }
            function fresh(gate: Result<bool, i32>.Err): Result<bool, i32> {
                let _ = both(gate?, (((fail<bool, i32>(7i32)?) |> relay?) |> known?));
                return Result.Ok(true);
            }
            function reachedGeneric(gate: Result<bool, i32>.Ok): Result<bool, i32> {
                let _ = both(gate?, ((successValue<bool, i32>(true)?) |> relay?));
                return Result.Ok(true);
            }
            function skippedGeneric(gate: Result<bool, i32>.Err): Result<bool, i32> {
                let _ = both(gate?, ((successValue<bool, i32>(true)?) |> relay?));
                return Result.Ok(true);
            }
            function reachedRetained(gate: Result<bool, i32>.Ok): Result<bool, i32> {
                let _ = both(gate?, ((fail<bool, i32, i32>(7i32, cause())?) |> relay?));
                return Result.Ok(true);
            }
            function skippedRetained(gate: Result<bool, i32>.Err): Result<bool, i32> {
                let _ = both(gate?, ((fail<bool, i32, i32>(7i32, cause())?) |> relay?));
                return Result.Ok(true);
            }
            function reachedConditional(gate: Result<bool, i32>.Ok, bit: bool): Result<bool, i32> {
                let _ = both(gate?, (((if (bit) successValue<bool, i32>(true) else successValue<bool, i32>(false))?) |> relay?) |> known?);
                return Result.Ok(true);
            }
            function skippedConditional(gate: Result<bool, i32>.Err, bit: bool): Result<bool, i32> {
                let _ = both(gate?, (((if (bit) successValue<bool, i32>(true) else successValue<bool, i32>(false))?) |> relay?) |> known?);
                return Result.Ok(true);
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
            let lower =
                |name| lower_body(receipt, module, id(name), AnalysisLimits::default()).unwrap();
            let role = typed.result_role().unwrap();
            let deep = lower("deep");
            let mir = deep
                .mir()
                .expect("whole-pipe flow preserves failures of known-Ok steps' inputs");
            let (yes, no) = variant_edges(mir, mir.entry, &role.ok.id, b"input");
            let relay = b"((input?) |> known?) |> relay?";
            let (relay_yes, relay_no) = variant_edges(mir, yes, &role.ok.id, relay);
            assert_forward(&linear(mir, no), role, b"input", &[b"keep: string"]);
            assert_forward(&linear(mir, relay_no), role, relay, &[b"keep: string"]);
            assert_return(
                &linear(mir, relay_yes),
                &role.ok.id,
                true,
                &[b"keep: string"],
            );
            assert!(calls_on_path(mir, [false]).is_empty());
            assert_eq!(
                calls_on_path(mir, [true, false]),
                [id("known"), id("relay")]
            );
            assert_eq!(
                calls_on_path(mir, [true, true]),
                [id("known"), id("relay"), id("known")]
            );
            for name in ["canonical", "fresh"] {
                let report = lower(name);
                let mir = report
                    .mir()
                    .expect("supported producer needs neither a call nor a carrier when skipped");
                assert!(calls_on_path(mir, []).is_empty());
                assert_forward(&linear(mir, mir.entry), role, b"gate", &[]);
                assert!(!linear(mir, mir.entry).into_iter().any(|op| matches!(
                    op,
                    Operation::ConstructResultError { .. } | Operation::ConstructVariant { .. }
                )));
            }
            for (name, producer) in [
                (
                    "reachedGeneric",
                    b"successValue<bool, i32>(true)".as_slice(),
                ),
                ("skippedGeneric", b"successValue<bool, i32>(true)"),
                ("reachedRetained", b"fail<bool, i32, i32>(7i32, cause())"),
                ("skippedRetained", b"fail<bool, i32, i32>(7i32, cause())"),
                (
                    "reachedConditional",
                    b"if (bit) successValue<bool, i32>(true) else successValue<bool, i32>(false)",
                ),
                (
                    "skippedConditional",
                    b"if (bit) successValue<bool, i32>(true) else successValue<bool, i32>(false)",
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none(), "{name}");
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    producer,
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn discarded_compound_call_cleans_only_the_materialized_success_result() {
    use compiler::analysis::{
        TypeTerm,
        scalars::{NodeFactKind, data::DataExpression},
    };

    probe_with_typed(
        "propagation::discarded_compound_call_cleans_only_the_materialized_success_result",
        r#"
            function compound(bit: bool): [string, string] { return ["left", "right"]; }
            function after(): bool { return true; }
            function discardPair(input: Option<bool>, keep: string): Option<bool> {
                compound(input?);
                let bit = after();
                return Option.Some(bit);
            }
            function groupedPair(input: Option<bool>, keep: string): Option<bool> {
                ((compound(input?)));
                let bit = after();
                return Option.Some(bit);
            }
        "#,
        |request, module, receipt, typed| {
            const CALL: &[u8] = b"compound(input?)";
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let node = |span| {
                module
                    .syntax
                    .nodes
                    .iter()
                    .position(|node| node.span.bytes() == span)
                    .unwrap()
            };
            let call = typed.bodies().node(module, node(CALL)).unwrap().unwrap();
            let NodeFactKind::Expression(expression) = &call.kind else {
                panic!("expected typed call facts");
            };
            assert!(matches!(expression.ty.as_ref(), TypeTerm::Tuple(leaves)
                if leaves.len() == 2 && leaves.iter().all(|ty|
                    matches!(ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "string"))));
            let application = expression.call.as_deref().unwrap();
            assert_eq!(application.callee, id("compound"));
            assert!(application.substitution.is_empty() && application.pipe_input.is_none());
            let [argument] = application.arguments.as_slice() else {
                panic!("expected one complete propagating argument");
            };
            let argument = typed.bodies().node(module, *argument).unwrap().unwrap();
            assert_eq!(argument.site.span.bytes(), b"input?");
            assert!(matches!(&argument.kind, NodeFactKind::Expression(facts)
                if matches!(&facts.data, Some(DataExpression::Propagation(branch))
                    if branch.can_continue && branch.can_return)));
            let lower =
                |name| lower_body(receipt, module, id(name), AnalysisLimits::default()).unwrap();
            let producer = lower("compound");
            assert!(
                producer.mir().is_some(),
                "tuple producer: {:?}",
                producer.completion()
            );
            let report = lower("discardPair");
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "test_mode={}: {:?} at {:?}",
                    request.test_mode,
                    report.completion(),
                    report
                        .blocked_at()
                        .map(|site| site.span.bytes().escape_ascii().to_string())
                )
            });
            let option = typed.option_role().unwrap();
            let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
            let prefix = straight_blocks(mir, mir.entry);
            assert!(!prefix.iter().any(|block| matches!(
                &block.operation,
                Operation::Drop(_) | Operation::Cleanup(_) | Operation::CopyOptionPayload { .. }
            )));
            let Operation::TestVariant {
                operand: carrier, ..
            } = &prefix.last().unwrap().operation
            else {
                panic!("expected the input carrier test before either exit");
            };
            assert_eq!(calls_on_path(mir, [true]), [id("compound"), id("after")]);
            assert!(calls_on_path(mir, [false]).is_empty());
            assert!(
                !linear(mir, no)
                    .iter()
                    .any(|op| matches!(op, Operation::CopyOptionPayload { .. }))
            );
            assert_return(&linear(mir, no), &option.none.id, false, &[b"keep: string"]);
            assert!(!linear(mir, no).iter().any(|op| matches!(op,
                Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action,
                    Obligation::EndStorage(place) if place.root.node == call.site.node)))));

            let success = straight_blocks(mir, yes);
            let called = success.iter().position(|b| matches!(&b.operation,
                Operation::Call(callee) if *callee == id("compound") && b.site.node == call.site.node)).unwrap();
            let extracted: Vec<_> = success
                .iter()
                .enumerate()
                .filter_map(|(i, b)| match &b.operation {
                    Operation::CopyOptionPayload { operand, into } => Some((i, operand, into)),
                    _ => None,
                })
                .collect();
            assert!(
                matches!(extracted.as_slice(), [(i, operand, into)] if *i < called
                && operand.root.node == carrier.root.node && operand.projection.is_none()
                && into.root.span.bytes() == b"input?" && into.projection.is_none())
            );
            let after = success
                .iter()
                .position(|b| {
                    matches!(&b.operation,
                Operation::Call(callee) if *callee == id("after"))
                })
                .unwrap();
            let mut drops = Vec::new();
            for (index, block) in success.iter().enumerate() {
                let mut record = |place: &compiler::analysis::ownership::Place<'_>| {
                    if place.root.node == call.site.node {
                        assert!(called < index && index < after);
                        drops.push(place.tuple_element());
                    }
                };
                match &block.operation {
                    Operation::Drop(place) => record(place),
                    Operation::Cleanup(actions) => {
                        for action in actions {
                            if let Obligation::Drop(place) = action {
                                record(place);
                            }
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(drops, [Some(1), Some(0)]);
            let suffix: Vec<_> = success[after..]
                .iter()
                .map(|block| &block.operation)
                .collect();
            assert_return(&suffix, &option.some.id, true, &[b"keep: string"]);
            let exact = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                max_depth: report.statistics().max_depth,
            };
            let bounded = |limits| lower_body(receipt, module, id("discardPair"), limits).unwrap();
            assert!(bounded(exact).mir().is_some());
            assert!(lower("groupedPair").mir().is_some());
            let depth = AnalysisLimits {
                max_depth: exact.max_depth,
                ..AnalysisLimits::default()
            };
            let grouped = lower_body(receipt, module, id("groupedPair"), depth).unwrap();
            assert_eq!(grouped.completion(), Completion::ResourceLimit);
            assert!(grouped.mir().is_none());
            for short in [
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
            ] {
                let limited = bounded(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn nonpropagating_named_text_pair_keeps_the_ordinary_work_limit() {
    probe(
        "propagation::nonpropagating_named_text_pair_keeps_the_ordinary_work_limit",
        "function pair(bit: bool): [string, string] { return [\"left\", \"right\"]; }\nfunction plain(bit: bool): void { let output = pair(bit); }\n",
        |_, module, receipt| {
            let decl = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("plain"))
                .unwrap();
            // Measured at ddb5df7; there is no propagating argument to check.
            let exact = AnalysisLimits {
                max_work: 4887,
                max_entries: 394,
                ..Default::default()
            };
            let result = lower_body(receipt, module, &decl.id, exact).unwrap();
            assert_eq!(result.completion(), Completion::Checked);
            assert!(result.mir().is_some());
        },
    );
}

#[test]
fn named_compound_call_keeps_the_result_until_lexical_exit() {
    fn calls<'g>(ops: &[&Operation<'g>]) -> Vec<(usize, &'g loader::DeclarationId)> {
        ops.iter()
            .enumerate()
            .filter_map(|(index, op)| match op {
                Operation::Call(callee) | Operation::CallIntoTemporary { callee, .. } => {
                    Some((index, *callee))
                }
                _ => None,
            })
            .collect()
    }

    let mut unmet = Vec::new();
    probe_with_typed(
        "propagation::named_compound_call_keeps_the_result_until_lexical_exit",
        r#"
            function compound(bit: bool): [string, string] { return ["left", "right"]; }
            function after(): bool { return true; }
            function named(input: Option<bool>, keep: string): Option<bool> {
                let output = compound(input?);
                let bit = after();
                return Option.Some(bit);
            }
            function reused(input: Option<bool>): Option<bool> {
                let output = compound(input?);
                let taken = output;
                let again = output;
                return Option.Some(true);
            }
        "#,
        |request, module, receipt, typed| {
            let id = |name| {
                &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id
            };
            let lower =
                |name| lower_body(receipt, module, id(name), AnalysisLimits::default()).unwrap();
            let producer = lower("compound");
            assert_eq!(producer.completion(), Completion::Checked);
            assert!(producer.mir().is_some(), "the tuple constructor must lower");

            let named = lower("named");
            let reused = lower("reused");
            // Report both modes before failing an unmet completion judgment.
            for (name, report, expected) in [
                ("named", &named, Completion::Checked),
                ("reused", &reused, Completion::SourceRejected),
            ] {
                let actual = report.completion();
                let blocker = report
                    .blocked_at()
                    .map(|site| site.span.bytes().escape_ascii().to_string());
                eprintln!(
                    "test_mode={}: M4={:?}; producer M5={:?}; {name} M5={actual:?}, blocker={blocker:?}, MIR={}, findings={}",
                    request.test_mode,
                    typed.completion(),
                    producer.completion(),
                    report.mir().is_some(),
                    report.findings().len(),
                );
                if actual != expected {
                    unmet.push((request.test_mode, name, expected, actual, blocker));
                }
            }

            if let Some(mir) = named.mir() {
                let option = typed.option_role().unwrap();
                let (yes, no) = variant_edges(mir, mir.entry, &option.some.id, b"input");
                let prefix = straight_blocks(mir, mir.entry);
                let path = |edge| {
                    prefix
                        .iter()
                        .map(|block| &block.operation)
                        .chain(linear(mir, edge))
                        .collect::<Vec<_>>()
                };
                let success = path(yes);
                let failure = path(no);
                let success_calls = calls(&success);
                let [(called, producer), (after, next)] = success_calls.as_slice() else {
                    panic!("expected one producer call and one next statement: {success_calls:?}");
                };
                assert_eq!([*producer, *next], [id("compound"), id("after")]);
                let Operation::TestVariant {
                    operand: carrier, ..
                } = &prefix.last().unwrap().operation
                else {
                    panic!("expected the input carrier test before either exit");
                };
                let staged: Vec<_> = success[..*called]
                    .iter()
                    .filter_map(|op| match op {
                        Operation::CopyOptionPayload { operand, into } => Some((operand, into)),
                        _ => None,
                    })
                    .collect();
                assert!(matches!(staged.as_slice(), [(operand, into)]
                    if operand.root.node == carrier.root.node && operand.projection.is_none()
                    && into.root.span.bytes() == b"input?" && into.projection.is_none()));
                assert!(calls(&failure).is_empty());
                assert!(!failure.iter().any(|op| matches!(
                    op,
                    Operation::CopyOptionPayload { .. } | Operation::Initialize(_)
                )));
                let export = success
                    .iter()
                    .position(|op| matches!(op, Operation::Export(_)))
                    .unwrap();
                assert!(*after < export);
                assert_return(
                    &success,
                    &option.some.id,
                    true,
                    &[b"output", b"output", b"keep: string"],
                );
                let leaves: Vec<_> = path_drops(mir, yes)
                    .into_iter()
                    .filter_map(|action| match action {
                        Obligation::Drop(place) if place.root.span.bytes() == b"output" => {
                            Some((place.root.node, place.tuple_element()))
                        }
                        _ => None,
                    })
                    .collect();
                assert!(
                    matches!(leaves.as_slice(), [(high, Some(1)), (low, Some(0))] if high == low)
                );

                assert_return(&failure, &option.none.id, false, &[b"keep: string"]);
                assert!(!failure.iter().any(|op| matches!(op, Operation::Cleanup(actions)
                    if actions.iter().any(|action| matches!(action, Obligation::EndStorage(place)
                        if [b"output".as_slice(), b"compound(input?)"].contains(&place.root.span.bytes()))))));

                let exact = AnalysisLimits {
                    max_work: named.statistics().work,
                    max_entries: named.statistics().entries,
                    max_depth: named.statistics().max_depth,
                };
                let bounded = |limits| lower_body(receipt, module, id("named"), limits).unwrap();
                assert!(bounded(exact).mir().is_some());
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
                    let limited = bounded(short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none());
                }
            } else {
                assert_ne!(named.completion(), Completion::Checked);
            }

            assert!(reused.mir().is_none());
            if reused.completion() == Completion::SourceRejected {
                let [finding] = reused.findings() else {
                    panic!("expected one moved-result finding");
                };
                assert_eq!(finding.code(), "R0200");
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| &d.id == id("reused"))
                    .unwrap();
                let nodes = &module.syntax.nodes;
                let [_, taken, again, _] = nodes[nodes[decl.node].children[1]].children[..] else {
                    panic!("expected two uses after the initializer")
                };
                for (site, statement, written) in [
                    (finding.site, again, b"let again = output;".as_slice()),
                    (finding.related, taken, b"let taken = output;"),
                ] {
                    let operand = *nodes[statement].children.last().unwrap();
                    assert_eq!(nodes[statement].span.bytes(), written);
                    assert_eq!(
                        (site.module, site.node, site.span),
                        (&module.key, operand, &nodes[operand].span)
                    );
                    assert_eq!(site.span.bytes(), b"output");
                }
            }
        },
    );
    assert!(
        unmet.is_empty(),
        "named compound result judgments: {unmet:?}"
    );
}

#[test]
fn named_compound_call_checks_only_reached_ownership() {
    probe_with_typed(
        "propagation::named_compound_call_checks_only_reached_ownership",
        r#"
            function compound(bit: bool): [string, string] { return ["left", "right"]; }
            function broken(bit: bool): [string, string] {
                let text = "owned";
                dispose(text);
                dispose(text);
                return ["left", "right"];
            }
            function gate(text: string): Option<bool>.None { return Option<bool>.None; }
            function gateLive(text: string): Option<bool> { return Option.Some(true); }
            function read(view: &string): bool { return true; }
            function after(): bool { return true; }
            function transferred(input: Option<bool>.Some): Option<bool> {
                let output = compound(input?);
                let taken = output;
                return Option.Some(after());
            }
            function stopped(input: Option<bool>.None, keep: string): Option<bool> {
                let output = broken(input?);
                let taken = output;
                return Option.Some(after());
            }
            function reached(input: Option<bool>.Some): Option<bool> {
                let output = broken(input?);
                return Option.Some(true);
            }
            function moved(text: string): Option<bool> {
                dispose(text);
                let output = compound(gate(text)?);
                return Option.Some(true);
            }
            function loaned(text: string): Option<bool> {
                let view = &text;
                let output = compound(gateLive(text)?);
                return Option.Some(read(view));
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
            let lower =
                |name| lower_body(receipt, module, id(name), AnalysisLimits::default()).unwrap();
            let option = typed.option_role().unwrap();
            let transferred = lower("transferred");
            let mir = transferred
                .mir()
                .expect("known Some transfers the named owner");
            assert_eq!(calls_on_path(mir, []), [id("compound"), id("after")]);
            let ops = linear(mir, mir.entry);
            let compound = ops
                .iter()
                .position(|op| matches!(op, Operation::Call(callee) if *callee == id("compound")))
                .unwrap();
            let after = ops
                .iter()
                .position(|op| matches!(op, Operation::Call(callee) if *callee == id("after")))
                .unwrap();
            let transfers: Vec<_> = ops
                .iter()
                .enumerate()
                .filter_map(|(index, op)| match op {
                    Operation::Move { from, into } if from.root.span.bytes() == b"output" => {
                        Some((index, from, into))
                    }
                    _ => None,
                })
                .collect();
            assert!(matches!(transfers.as_slice(), [(index, from, Some(into))]
                if compound < *index && *index < after && into.root.span.bytes() == b"taken"
                && from.projection.is_none() && into.projection.is_none()));
            assert_return(&ops, &option.some.id, true, &[b"taken", b"taken"]);
            let drops: Vec<_> = path_drops(mir, mir.entry)
                .into_iter()
                .map(|drop| match drop {
                    Obligation::Drop(place) => place.tuple_element(),
                    _ => panic!("expected tuple member cleanup"),
                })
                .collect();
            assert_eq!(drops, [Some(1), Some(0)]);

            let stopped = lower("stopped");
            let mir = stopped
                .mir()
                .expect("known None skips the invalid producer and suffix");
            assert!(calls_on_path(mir, []).is_empty());
            let ops = linear(mir, mir.entry);
            assert_return(&ops, &option.none.id, false, &[b"keep: string"]);
            assert!(!ops.iter().any(|op| matches!(op,
                Operation::Initialize(_) | Operation::CopyOptionPayload { .. } | Operation::Move { .. })
                || matches!(op, Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action,
                    Obligation::EndStorage(place) if [b"output".as_slice(), b"taken", b"broken(input?)"].contains(&place.root.span.bytes()))))));

            for (name, site) in [("reached", b"dispose(text)".as_slice()), ("moved", b"text")] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected one moved-use finding for {name}");
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), b"dispose(text)");
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            let loaned = lower("loaned");
            assert_eq!(loaned.completion(), Completion::SourceRejected);
            assert!(loaned.mir().is_none());
            let [finding] = loaned.findings() else {
                panic!("expected one conflict with the live loan");
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"text");
            assert_eq!(finding.related.span.bytes(), b"&text");
        },
    );
}

#[test]
fn discarded_compound_call_checks_only_reached_ownership() {
    probe_with_typed(
        "propagation::discarded_compound_call_checks_only_reached_ownership",
        r#"
            function broken(bit: bool): [string, string] {
                let text = "owned";
                dispose(text);
                dispose(text);
                return ["left", "right"];
            }
            function compound(bit: bool): [string, string] { return ["left", "right"]; }
            function gate(text: string): Option<bool>.None { return Option<bool>.None; }
            function after(): bool { return true; }
            function stopped(input: Option<bool>.None, keep: string): Option<bool> {
                broken((input?));
                return Option.Some(after());
            }
            function ready(input: Option<bool>.Some): Option<bool> {
                compound(input?);
                return Option.Some(after());
            }
            function reached(input: Option<bool>.Some): Option<bool> {
                broken(input?);
                return Option.Some(true);
            }
            function moved(text: string): Option<bool> {
                dispose(text);
                compound(gate(text)?);
                return Option.Some(true);
            }
            function scalar(bit: bool): bool { return bit; }
            function stoppedScalar(input: Option<bool>.None, keep: string): Option<bool> {
                scalar(input?);
                return Option.Some(after());
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
            let lower =
                |name| lower_body(receipt, module, id(name), AnalysisLimits::default()).unwrap();
            let stopped = lower("stopped");
            let mir = stopped
                .mir()
                .expect("known None skips the invalid producer");
            assert!(calls_on_path(mir, []).is_empty());
            assert_return(
                &linear(mir, mir.entry),
                &typed.option_role().unwrap().none.id,
                false,
                &[b"keep: string"],
            );
            let scalar = lower("stoppedScalar");
            // Disposer selection itself is metered; the resulting MIR has
            // no extra tuple storage, cleanup, or call blocks.
            assert_eq!(mir.blocks.len(), scalar.mir().unwrap().blocks.len());
            assert!(!linear(mir, mir.entry).iter().any(|op| matches!(op,
                Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action,
                    Obligation::EndStorage(place) if place.root.span.bytes() == b"broken((input?))")))));
            let ready = lower("ready");
            let mir = ready.mir().expect("known Some reaches the valid producer");
            assert_eq!(calls_on_path(mir, []), [id("compound"), id("after")]);
            let path = linear(mir, mir.entry);
            assert!(
                !path
                    .iter()
                    .any(|op| matches!(op, Operation::TestVariant { .. }))
            );
            let produced = path
                .iter()
                .position(|op| matches!(op, Operation::Call(callee) if *callee == id("compound")))
                .unwrap();
            assert!(
                matches!(&path[produced + 1..], [Operation::Cleanup(actions), Operation::Call(after), ..]
                if *after == id("after") && matches!(actions.as_slice(), [Obligation::Drop(right), Obligation::Drop(left), Obligation::EndStorage(root)]
                    if root.root.span.bytes() == b"compound(input?)" && root.projection.is_none()
                    && right.root.node == root.root.node && left.root.node == root.root.node
                    && right.tuple_element() == Some(1) && left.tuple_element() == Some(0)))
            );
            for (name, site) in [("reached", b"dispose(text)".as_slice()), ("moved", b"text")] {
                let report = lower(name);
                assert_eq!(
                    report.completion(),
                    Completion::SourceRejected,
                    "{name}: {:?}",
                    report.blocked_at()
                );
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected one moved-use finding for {name}")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), b"dispose(text)");
                assert!(finding.related.span.start() < finding.site.span.start());
            }
        },
    );
}

#[test]
fn compound_propagating_results_stay_outside_other_call_routes() {
    probe(
        "propagation::compound_propagating_results_stay_outside_other_call_routes",
        r#"
            function compound(bit: bool): [string, string] { return ["left", "right"]; }
            function generic<T>(bit: bool): [string, string] { return ["left", "right"]; }
            function mixed(bit: bool): [string, bool] { return ["left", bit]; }
            function fromText(text: string): [string, string] { return [text, "right"]; }
            function withBorrow(bit: bool, text: &string): [string, string] { return ["left", "right"]; }
            function withGuard(bit: bool, lease: LockGuard<i32>): [string, string] { return ["left", "right"]; }
            function gate(bit: bool): Option<bool> { return Option.Some(bit); }
            function wildcard(input: Option<bool>): Option<bool> {
                let _ = compound(input?);
                return Option.Some(true);
            }
            function piped(input: Option<bool>): Option<bool> {
                (input?) |> compound;
                return Option.Some(true);
            }
            function returned(input: Option<bool>): Option<[string, string]> {
                return Option.Some(compound(input?));
            }
            function embedded(input: Option<bool>): Option<bool> {
                compound(gate(input?)?);
                return Option.Some(true);
            }
            function skippedGeneric(input: Option<bool>.None): Option<bool> {
                generic<bool>(input?);
                return Option.Some(true);
            }
            function mixedResult(input: Option<bool>): Option<bool> {
                mixed(input?);
                return Option.Some(true);
            }
            function textCarrier(input: Option<string>): Option<bool> {
                fromText(input?);
                return Option.Some(true);
            }
            function resultCarrier(input: Result<bool, i32>): Result<bool, i32> {
                compound(input?);
                return Result.Ok(true);
            }
            function borrowed(input: Option<bool>, keep: string): Option<bool> {
                withBorrow(input?, &keep);
                return Option.Some(true);
            }
            function protectedArg(input: Option<bool>, lease: LockGuard<i32>): Option<bool> {
                withGuard(input?, lease);
                return Option.Some(true);
            }
            function namedPiped(input: Option<bool>): Option<bool> {
                let output = (input?) |> compound;
                return Option.Some(true);
            }
            function namedEmbedded(input: Option<bool>): Option<bool> {
                let output = compound(gate(input?)?);
                return Option.Some(true);
            }
            function namedGeneric(input: Option<bool>.None): Option<bool> {
                let output = generic<bool>(input?);
                return Option.Some(true);
            }
            function namedMixed(input: Option<bool>): Option<bool> {
                let output = mixed(input?);
                return Option.Some(true);
            }
            function namedBorrowed(input: Option<bool>, keep: string): Option<bool> {
                let output = withBorrow(input?, &keep);
                return Option.Some(true);
            }
        "#,
        |_, module, receipt| {
            for (name, site) in [
                ("wildcard", b"compound(input?)".as_slice()),
                ("piped", b"(input?) |> compound"),
                ("returned", b"Option.Some(compound(input?))"),
                ("embedded", b"compound(gate(input?)?)"),
                ("skippedGeneric", b"generic<bool>(input?)"),
                ("mixedResult", b"mixed(input?)"),
                ("textCarrier", b"fromText(input?)"),
                ("resultCarrier", b"compound(input?)"),
                ("borrowed", b"withBorrow(input?, &keep)"),
                ("protectedArg", b"withGuard(input?, lease)"),
                ("namedPiped", b"(input?) |> compound"),
                ("namedEmbedded", b"compound(gate(input?)?)"),
                ("namedGeneric", b"generic<bool>(input?)"),
                ("namedMixed", b"mixed(input?)"),
                ("namedBorrowed", b"withBorrow(input?, &keep)"),
            ] {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &declaration.id, AnalysisLimits::default())
                        .unwrap();
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site, "{name}");
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
            Terminator::FinishStartup | Terminator::FinishExpectation => {
                panic!("expected a callable return")
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

pub(super) fn assert_return(
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
    assert_export_with_option(ops, expected_drops, None, defines_output);
}

fn assert_option_cleanup_return(
    ops: &[&Operation<'_>],
    option: &compiler::analysis::typed::OptionRole<'_>,
    expected_drops: &[&[u8]],
) {
    let carrier = expected_drops
        .last()
        .expect("expect one lexical carrier after the staged drops");
    assert_export_with_option(
        ops,
        expected_drops,
        Some((&option.some.id, carrier)),
        |op, output| {
            matches!(op, Operation::ConstructVariant { variant, payload: None, into }
            if *variant == &option.none.id && into.root.node == output.root.node)
        },
    );
}

fn assert_export_with_option<'g>(
    ops: &[&Operation<'g>],
    expected_drops: &[&[u8]],
    mut expected_carrier: Option<(&loader::DeclarationId, &[u8])>,
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
                        Obligation::DropOptionPayload { operand, some } => {
                            assert!(index > export, "export the value before exit cleanup");
                            assert_eq!(
                                expected_carrier.take(),
                                Some((*some, operand.root.span.bytes()))
                            );
                            drops.push(operand.root.span.bytes());
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
    assert!(
        expected_carrier.is_none(),
        "missing conditional Option cleanup"
    );
}
