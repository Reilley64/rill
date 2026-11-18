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
fn direct_calls_retain_compatible_inputs_not_the_access_loan() {
    probe(
        "retained::direct_calls_retain_compatible_inputs_not_the_access_loan",
        r#"
            function remember(slot: &mut &bool, input: &bool): void { *slot = input; }
            function clearLike(slot: &mut &bool, input: &bool): void { *slot = input; }
            function inReverse(input: &bool, slot: &mut &bool): void { *slot = input; }
            function broken(slot: &mut &bool, input: &bool): void {
                let inner = true;
                *slot = &inner;
            }
            function valid(seed: &bool, flag: bool): bool {
                let mut out = seed;
                remember(&mut out, &flag);
                return *out;
            }
            function expires(seed: &bool): bool {
                let mut out = seed;
                { let flag = true; remember(&mut out, &flag); }
                return *out;
            }
            function maybeExpires(seed: &bool, choose: bool): bool {
                let mut out = seed;
                { let flag = true; if (choose) { remember(&mut out, &flag); } }
                return *out;
            }
            function reverse(seed: &bool, flag: bool): bool {
                let mut out = seed;
                inReverse(&flag, &mut out);
                return *out;
            }
            function keepsOld(seed: &bool): bool {
                let mut out = seed;
                { let old = true; out = &old; clearLike(&mut out, seed); }
                return *out;
            }
            function replaced(seed: &bool): bool {
                let mut out = seed;
                { let flag = true; remember(&mut out, &flag); out = seed; }
                return *out;
            }
            function dead(seed: &bool): bool {
                let mut out = seed;
                { let flag = true; remember(&mut out, &flag); }
                return false;
            }
            function checkedCallee(seed: &bool): bool {
                let mut out = seed;
                broken(&mut out, seed);
                return *out;
            }
        "#,
        |_, module, receipt| {
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
            let remember = &declaration("remember").id;
            let valid = lower("valid");
            let mir = valid
                .mir()
                .unwrap_or_else(|| panic!("valid: {:?}", valid.completion()));
            let events: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. }
                            | Operation::CallRetaining { .. }
                            | Operation::Dereference(_)
                    )
                })
                .collect();
            let [
                Operation::Borrow {
                    source: dest,
                    exclusive: true,
                    ..
                },
                Operation::Borrow {
                    source,
                    holder,
                    exclusive: false,
                },
                Operation::CallRetaining {
                    callee,
                    destination,
                    input,
                },
                Operation::Dereference(read),
            ] = events.as_slice()
            else {
                panic!("expected access, input, retention and read: {events:?}")
            };
            assert_eq!(*callee, remember);
            assert_eq!(dest.root.span.bytes(), b"out");
            assert_eq!(source.root.span.bytes(), b"flag: bool");
            assert_eq!(destination.root.node, dest.root.node);
            assert_eq!(read.root.node, dest.root.node);
            assert_eq!(input.root.node, holder.root.node);

            let reverse = lower("reverse");
            let mir = reverse.mir().expect("same contract with reversed formals");
            assert!(linear(mir, mir.entry).iter().any(|op| matches!(op,
                Operation::CallRetaining { callee, destination, input }
                if *callee == &declaration("inReverse").id && destination.root.span.bytes() == b"out" && input.root.span.bytes() == b"&flag")));

            for name in ["replaced", "dead"] {
                let report = lower(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                assert!(linear(mir, mir.entry).iter().any(|op| matches!(op,
                    Operation::CallRetaining { callee, destination, .. }
                    if *callee == remember && destination.root.span.bytes() == b"out")));
            }
            for (name, code, source) in [
                ("expires", "R0203", b"&flag".as_slice()),
                ("maybeExpires", "R0203", b"&flag".as_slice()),
                ("keepsOld", "R0203", b"&old".as_slice()),
                ("checkedCallee", "R0202", b"&inner".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected diagnostic")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.related.span.bytes(), source, "{name}");
            }
            let limits = AnalysisLimits {
                max_work: valid.statistics().work,
                max_entries: valid.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(check("valid", limits).mir().is_some());
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
                let limited = check("valid", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
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
