use super::*;
use compiler::analysis::ownership::Place;

#[test]
fn caller_owned_descriptor_cannot_retain_body_local_sources() {
    probe(
        "retained::caller_owned_descriptor_cannot_retain_body_local_sources",
        r#"
            function remember(slot: &mut &bool, input: &bool, text: string): void {
                *slot = input;
            }
            function restored(slot: &mut &bool, input: &bool): void {
                {
                    let flag = true;
                    *slot = &flag;
                    (*slot) = input;
                }
            }
            function escaped(slot: &mut &bool): void {
                let flag = true;
                *slot = &flag;
            }
            function movedSlot(slot: &mut &bool, input: &bool): void {
                let moved = slot;
                *slot = input;
            }
            function check(value: &mut bool): bool { return *value; }
            function beforeOverwrite(slot: &mut &bool, input: &bool): void {
                let mut flag = true;
                *slot = &flag;
                if (check(&mut flag)) { *slot = input; } else { *slot = input; }
            }
            function scopeBeforeOverwrite(slot: &mut &bool, input: &bool): void {
                { let flag = true; *slot = &flag; }
                *slot = input;
            }
            function returned(slot: &mut &bool, flag: bool): void {
                if (true) { *slot = &flag; return; }
            }
            function split(slot: &mut &bool, input: &bool, pick: bool): void {
                let flag = true;
                if (pick) { *slot = &flag; }
                else { *slot = input; }
            }
        "#,
        |_, module, receipt| {
            let check = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let lower = |name| check(name, AnalysisLimits::default());
            let remembered = lower("remember");
            let mir = remembered
                .mir()
                .unwrap_or_else(|| panic!("remember: {:?}", remembered.completion()));
            let trace: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::InstallThrough { .. }
                            | Operation::Cleanup(_)
                    )
                })
                .collect();
            let [
                Operation::Copy {
                    from,
                    into: Some(staged),
                },
                Operation::InstallThrough {
                    descriptor,
                    incoming,
                },
                Operation::Cleanup(cleanup),
            ] = trace.as_slice()
            else {
                panic!("expected stage, store, and cleanup on one path: {trace:?}");
            };
            assert_eq!(from.root.span.bytes(), b"input: &bool");
            assert_eq!(staged.root.node, incoming.root.node);
            assert_eq!(descriptor.root.span.bytes(), b"slot: &mut &bool");
            assert!(cleanup.iter().any(|op| matches!(op, Obligation::Drop(text) if text.root.span.bytes() == b"text: string")));

            let limits = AnalysisLimits {
                max_work: remembered.statistics().work,
                max_entries: remembered.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(check("remember", limits).mir().is_some());
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
                let limited = check("remember", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

            let restored = lower("restored");
            let mir = restored
                .mir()
                .expect("exact overwrite retires the local backing before it ends");
            let writes = writes_on_path(mir);
            assert!(
                matches!(writes.as_slice(), [(first, local), (second, input)]
                if first.root.node == second.root.node && local.root.span.bytes() == b"&flag" && input.root.span.bytes() == b"input")
            );

            for (name, code, source) in [
                ("beforeOverwrite", "R0201", b"&flag".as_slice()),
                ("scopeBeforeOverwrite", "R0203", b"&flag".as_slice()),
                ("escaped", "R0202", b"&flag".as_slice()),
                ("split", "R0202", b"&flag".as_slice()),
                ("returned", "R0202", b"&flag".as_slice()),
                ("movedSlot", "R0200", b"slot".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.related.span.bytes(), source, "{name}");
                if code == "R0202" {
                    assert_eq!(finding.site.span.bytes(), b"*slot = &flag;");
                }
            }
        },
    );
}

#[test]
fn retained_write_handoff_does_not_grant_captured_or_result_storage() {
    use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};

    probe_report(
        "retained::retained_write_handoff_does_not_grant_captured_or_result_storage",
        r#"
            function captured(slot: &mut &bool, input: &bool): mut () => void {
                return () => { *slot = input; };
            }
            function unknown(slot: &mut Result<bool, i32>, incoming: Result<bool, i32>): void {
                *slot = incoming;
            }
        "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.core_blockers().is_empty());
            assert!(typed.receipt().is_none());
            let pending = typed.result_use().expect("Result-use evidence").pending();
            for store in [b"*slot = input;".as_slice(), b"*slot = incoming;"] {
                assert!(
                    pending
                        .iter()
                        .any(|p| p.blocker == ResultUseBlocker::Storage
                            && p.site.span.bytes() == store),
                    "missing blocker for {store:?}"
                );
            }
        },
    );
}

fn writes_on_path<'g>(mir: &Mir<'g>) -> Vec<(Place<'g>, Place<'g>)> {
    linear(mir, mir.entry)
        .into_iter()
        .filter_map(|op| match *op {
            Operation::InstallThrough {
                descriptor,
                incoming,
            } => Some((descriptor, incoming)),
            _ => None,
        })
        .collect()
}
