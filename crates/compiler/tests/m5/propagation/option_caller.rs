use super::*;
use compiler::analysis::{
    ownership::{DependencyContract, Place, analyze_ownership},
    scalars::BodyOwner,
};

const RELAY: &str = r#"
    function relay(input: Option<&string>): Option<&string> {
        let view = input?;
        return Option.Some(view);
    }
"#;

fn declaration<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
}

fn same(left: Place<'_>, right: Place<'_>) -> bool {
    left.projection.is_none() && right.projection.is_none() && left.root.node == right.root.node
}

#[test]
fn selected_shared_option_call_preserves_the_caller_backing() {
    let source = format!(
        "{RELAY}{}",
        r#"
        function caller(input: Option<&string>, tail: string): Option<&string> {
            let view = relay(input)?;
            let _ = input?;
            return Option.Some(view);
        }
    "#
    );
    probe_with_typed(
        "propagation::option_caller::selected_shared_option_call_preserves_the_caller_backing",
        &source,
        |_, module, receipt, typed| {
            let caller = declaration(module, "caller");
            let relay = declaration(module, "relay");
            let option = typed.option_role().unwrap();
            let defaults = AnalysisLimits::default();
            let report = lower_body(receipt, module, &caller.id, defaults).unwrap();
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "blocked at {:?}",
                    report.blocked_at().map(|s| s.span.bytes())
                )
            });
            let prefix = straight_blocks(mir, mir.entry);
            let [copy, invoke, test] = prefix.as_slice() else {
                panic!("copy, call and branch exactly once")
            };
            let Operation::Copy {
                from: input,
                into: Some(argument),
            } = copy.operation
            else {
                panic!("copy the named carrier into the call stage");
            };
            assert_eq!(input.root.span.bytes(), b"input: Option<&string>");
            assert!(!same(input, argument));
            let Operation::CallIntoTemporary {
                callee,
                arguments,
                into,
            } = &invoke.operation
            else {
                panic!("publish the selected call's staged result");
            };
            assert!(std::ptr::eq(*callee, &relay.id));
            assert!(matches!(arguments.as_slice(), [passed] if same(*passed, argument)));
            assert!(!same(*into, argument));
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, .. } if same(operand, *into))
            );
            let (some, none) = variant_edges(mir, mir.entry, &option.some.id, b"relay(input)");
            let success = straight_blocks(mir, some);
            assert!(
                matches!(success[0].operation, Operation::TakeOptionPayload { operand, into: view }
                if same(operand, *into) && view.root.span.bytes() == b"view")
            );
            assert!(
                matches!(success[1].operation, Operation::Copy { from, .. } if same(from, input))
            );
            let (reused, _) = variant_edges(mir, some, &option.some.id, b"input");
            assert_return(
                &linear(mir, reused),
                &option.some.id,
                true,
                &[b"tail: string"],
            );
            let failure = linear(mir, none);
            assert!(
                matches!(failure.as_slice(), [Operation::EndEmptyOption(operand),
                Operation::ConstructVariant { payload: None, .. }, Operation::Export(_), ..]
                if same(*operand, *into))
            );
            assert_return(&failure, &option.none.id, false, &[b"tail: string"]);
            assert!(mir.cancellation().is_none());

            let graph = analyze_ownership(receipt, defaults);
            let root = graph
                .roots()
                .iter()
                .find(|root| {
                    matches!(root.owner(),
                BodyOwner::Declaration(id) if std::ptr::eq(id, &caller.id))
                })
                .unwrap();
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.ordinary_mir().is_some());
            let [edge] = root.dependencies() else {
                panic!("one ordinary call")
            };
            assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
            assert!(std::ptr::eq(edge.declaration, &relay.id));
            assert_eq!(edge.site.span.bytes(), b"relay(input)");
        },
    );
}

#[test]
fn call_result_keeps_local_backing_until_the_descriptor_dies() {
    let source = format!(
        "{RELAY}{}",
        r#"
        function escape(local: string): Option<&string> {
            let input: Option<&string> = Option.Some(&local);
            let view = relay(input)?;
            return Option.Some(view);
        }
        function live(local: string): Option<&string> {
            let input: Option<&string> = Option.Some(&local);
            let view = relay(input)?;
            dispose(local);
            return Option.Some(view);
        }
        function ended(local: string): Option<bool> {
            let input: Option<&string> = Option.Some(&local);
            let _ = relay(input)?;
            dispose(local);
            return Option.Some(true);
        }
        function alwaysNone(input: Option<&string>): Option<&string> {
            return Option<&string>.None;
        }
        function conservative(local: string): Option<&string> {
            let input: Option<&string> = Option.Some(&local);
            let view = alwaysNone(input)?;
            dispose(local);
            return Option.Some(view);
        }
        function select(left: &string, right: &string): &string { return left; }
        function liveLeft(left: string, right: string): Option<&string> {
            let input: Option<&string> = Option.Some(select(&left, &right));
            let view = relay(input)?;
            dispose(left);
            return Option.Some(view);
        }
        function liveRight(left: string, right: string): Option<&string> {
            let input: Option<&string> = Option.Some(select(&left, &right));
            let view = relay(input)?;
            dispose(right);
            return Option.Some(view);
        }
    "#
    );
    probe_with_typed(
        "propagation::option_caller::call_result_keeps_local_backing_until_the_descriptor_dies",
        &source,
        |_, module, receipt, typed| {
            let check = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(module, name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            for (name, code, site, related) in [
                (
                    "escape",
                    "R0202",
                    b"return Option.Some(view);".as_slice(),
                    b"local: string".as_slice(),
                ),
                (
                    "live",
                    "R0203",
                    b"dispose(local)".as_slice(),
                    b"&local".as_slice(),
                ),
                (
                    "conservative",
                    "R0203",
                    b"dispose(local)".as_slice(),
                    b"&local".as_slice(),
                ),
                (
                    "liveLeft",
                    "R0203",
                    b"dispose(left)".as_slice(),
                    b"&left".as_slice(),
                ),
                (
                    "liveRight",
                    "R0203",
                    b"dispose(right)".as_slice(),
                    b"&right".as_slice(),
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
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
                let body = &module.syntax.nodes[declaration(module, name).node].span;
                assert!((body.start()..body.end()).contains(&finding.related.span.start()));
            }
            let ended = check("ended");
            let mir = ended.mir().expect("last use ended the borrowed dependency");
            let option = typed.option_role().unwrap();
            let (some, none) = variant_edges(mir, mir.entry, &option.some.id, b"relay(input)");
            let success = linear(mir, some);
            assert!(
                matches!(success.as_slice(), [Operation::TakeOptionPayload { into, .. },
                Operation::Cleanup(actions), Operation::Dispose { operand, .. }, ..]
                if operand.root.span.bytes() == b"local: string" && matches!(actions.as_slice(),
                    [Obligation::EndStorage(view)] if same(*view, *into)))
            );
            assert_return(&success, &option.some.id, true, &[]);
            let failure = linear(mir, none);
            assert!(
                !failure
                    .iter()
                    .any(|op| matches!(op, Operation::Dispose { .. }))
            );
            assert_return(&failure, &option.none.id, false, &[b"local: string"]);
        },
    );
}

#[test]
fn only_a_reached_selected_callee_needs_an_ordinary_body_proof() {
    probe(
        "propagation::option_caller::only_a_reached_selected_callee_needs_an_ordinary_body_proof",
        r#"
        function wrong(input: Option<&string>): Option<&string> {
            let owned = "text";
            return Option.Some(&owned);
        }
        function reached(input: Option<&string>): Option<bool> {
            let _ = wrong(input)?;
            return Option.Some(true);
        }
        function skipped(input: Option<&string>): Option<bool> {
            if (false) { let _ = wrong(input)?; }
            return Option.Some(true);
        }
        "#,
        |_, module, receipt| {
            let run = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(module, name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let reached = run("reached");
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("the callee's rejection")
            };
            assert_eq!(finding.code(), "R0202");
            assert_eq!(finding.site.span.bytes(), b"return Option.Some(&owned);");
            assert_eq!(finding.related.span.bytes(), b"owned");
            let skipped = run("skipped");
            assert_eq!(skipped.completion(), Completion::Checked);
            assert!(
                !skipped
                    .mir()
                    .unwrap()
                    .blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::CallIntoTemporary { .. }))
            );
        },
    );
}

#[test]
fn shared_option_postfix_call_does_not_open_other_call_contracts() {
    let test =
        "propagation::option_caller::shared_option_postfix_call_does_not_open_other_call_contracts";
    probe(
        test,
        r#"
        function choose(left: Option<&string>, right: Option<&string>): Option<&string> {
            let view = left?;
            return Option.Some(view);
        }
        function ambiguous(left: Option<&string>, right: Option<&string>): Option<&string> {
            let view = choose(left, right)?;
            return Option.Some(view);
        }
        "#,
        |_, module, receipt| {
            let d = declaration(module, "ambiguous");
            let report = lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap();
            assert_eq!(report.completion(), Completion::Incomplete);
            assert!(report.mir().is_none() && report.findings().is_empty());
            assert_eq!(
                report.blocked_at().unwrap().span.bytes(),
                b"choose(left, right)?"
            );
        },
    );
    probe_report(
        test,
        r#"
        function erased(input: Option<&string>, callback: (Option<&string>) => Option<&string>): Option<&string> {
            let view = callback(input)?;
            return Option.Some(view);
        }
        "#,
        |_, module, typed| {
            use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none() && typed.bodies().findings().is_empty());
            assert!(
                typed
                    .result_use()
                    .unwrap()
                    .pending()
                    .iter()
                    .any(|pending| pending.blocker == ResultUseBlocker::CallBoundary
                        && pending.site.module == &module.key
                        && pending.site.span.bytes() == b"callback(input)")
            );
        },
    );
    probe_report(
        test,
        r#"
        function store(slot: &mut Option<&string>, input: Option<&string>): Option<bool> {
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
