use super::*;
use compiler::analysis::ownership::{Place, RetainedPlace};

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
                    destination: RetainedPlace::Local(destination),
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
            let reverse_events: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. } | Operation::CallRetaining { .. }
                    )
                })
                .collect();
            assert!(matches!(reverse_events.as_slice(), [
                Operation::Borrow { source, holder, exclusive: false },
                Operation::Borrow { source: target, exclusive: true, .. },
                Operation::CallRetaining { callee, destination: RetainedPlace::Local(destination), input },
            ] if *callee == &declaration("inReverse").id && source.root.span.bytes() == b"flag: bool"
                && destination.root.span.bytes() == b"out" && target.root.node == destination.root.node
                && holder.root.node == input.root.node));

            for name in ["replaced", "dead"] {
                let report = lower(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                assert!(linear(mir, mir.entry).iter().any(|op| matches!(op,
                    Operation::CallRetaining { callee, destination: RetainedPlace::Local(destination), .. }
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
fn forwarding_reborrows_the_descriptor_and_keeps_its_contents() {
    probe(
        "retained::forwarding_reborrows_the_descriptor_and_keeps_its_contents",
        r#"
            function remember(slot: &mut &bool, input: &bool): void {
                let old = *slot;
                *slot = input;
            }
            function savedCopy(slot: &mut &bool, input: &bool): &bool {
                let flag = true;
                *slot = &flag;
                let old = *slot;
                *slot = input;
                return old;
            }
            function relay(slot: &mut &bool, input: &bool): void {
                remember((slot), input);
                *slot = input;
            }
            function readsIncoming(slot: &mut &bool, input: &bool): &bool {
                let old = *slot;
                *slot = input;
                return old;
            }
            function suspended(slot: &mut &bool, input: &bool): void {
                remember(slot, *slot);
            }
            function relayRestored(slot: &mut &bool, input: &bool): void {
                { let flag = true; relay(slot, &flag); *slot = input; }
            }
            function caller(seed: &bool, flag: bool): bool {
                let mut out = seed;
                relay(&mut out, &flag);
                return *out;
            }
            function callerExpires(seed: &bool): bool {
                let mut out = seed;
                { let flag = true; relay(&mut out, &flag); }
                return *out;
            }
            function escapesThroughRelay(slot: &mut &bool, input: &bool): void {
                let flag = true;
                relay(slot, &flag);
            }
            function expiresBeforeOverwrite(slot: &mut &bool, input: &bool): void {
                { let flag = true; relay(slot, &flag); }
                *slot = input;
            }
            function preservesOld(slot: &mut &bool, input: &bool): void {
                { let old = true; *slot = &old; relay(slot, input); }
            }
            function movedDescriptor(slot: &mut &bool, input: &bool): void {
                let moved = slot;
                relay(slot, input);
            }
            function aliased(seed: &bool): bool {
                let mut out = seed;
                let access = &mut out;
                remember(access, seed);
                return *out;
            }
            type Holder = { value: &bool; }
            function projected(seed: &bool): bool {
                let mut holder = Holder { value: seed };
                remember(&mut holder.value, seed);
                let saved = holder.value;
                return *saved;
            }
            function rejectedCaller(seed: &bool): bool {
                let mut out = seed;
                escapesThroughRelay(&mut out, seed);
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
            let remember = lower("remember");
            let remember_mir = remember
                .mir()
                .unwrap_or_else(|| panic!("remember: {:?}", remember.completion()));
            let reads_and_writes: Vec<_> = linear(remember_mir, remember_mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::CopyThrough { .. } | Operation::InstallThrough { .. }
                    )
                })
                .collect();
            assert!(matches!(reads_and_writes.as_slice(), [
                Operation::CopyThrough { descriptor: from, into: Some(old) },
                Operation::InstallThrough { descriptor: into, .. },
            ] if from.root.node == into.root.node && old.root.span.bytes() == b"old"));
            let saved = lower("savedCopy");
            assert_eq!(saved.completion(), Completion::SourceRejected);
            assert!(saved.mir().is_none());
            let [finding] = saved.findings() else {
                panic!("expected the copied local backing to escape")
            };
            assert_eq!(finding.code(), "R0202");
            assert_eq!(finding.site.span.bytes(), b"return old;");

            let incoming = lower("readsIncoming");
            let mir = incoming
                .mir()
                .expect("a copied caller-backed descriptor can be returned");
            assert!(
                linear(mir, mir.entry)
                    .iter()
                    .any(|op| matches!(op, Operation::Export(_)))
            );

            let relay = lower("relay");
            let mir = relay
                .mir()
                .unwrap_or_else(|| panic!("relay: {:?}", relay.completion()));
            let steps: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Reborrow { .. }
                            | Operation::CallRetaining { .. }
                            | Operation::InstallThrough { .. }
                    )
                })
                .collect();
            let [
                Operation::Reborrow {
                    parent,
                    exclusive: true,
                    ..
                },
                Operation::CallRetaining {
                    callee,
                    destination: RetainedPlace::ThroughParameter(destination),
                    input,
                },
                Operation::InstallThrough { descriptor, .. },
            ] = steps.as_slice()
            else {
                panic!("expected reborrow, retained call and resumed write: {steps:?}")
            };
            assert_eq!(*callee, &declaration("remember").id);
            assert_eq!(parent.root.span.bytes(), b"slot: &mut &bool");
            assert_eq!(parent.root.node, destination.root.node);
            assert_eq!(parent.root.node, descriptor.root.node);
            assert_eq!(input.root.span.bytes(), b"input");

            let caller = lower("caller");
            let mir = caller
                .mir()
                .expect("both the relay and its reached callee check");
            assert!(
                linear(mir, mir.entry).iter().any(|op| matches!(op,
                Operation::CallRetaining { callee, destination: RetainedPlace::Local(destination), .. }
                if *callee == &declaration("relay").id && destination.root.span.bytes() == b"out"))
            );
            let restored = lower("relayRestored");
            let mir = restored
                .mir()
                .expect("a resumed exact write can remove the retained source");
            assert!(
                matches!(writes_on_path(mir).as_slice(), [(_, input)] if input.root.span.bytes() == b"input")
            );

            for (name, code, cause) in [
                ("callerExpires", "R0203", b"&flag".as_slice()),
                ("suspended", "R0201", b"slot".as_slice()),
                ("escapesThroughRelay", "R0202", b"&flag".as_slice()),
                ("expiresBeforeOverwrite", "R0203", b"&flag".as_slice()),
                ("preservesOld", "R0203", b"&old".as_slice()),
                ("movedDescriptor", "R0200", b"slot".as_slice()),
                ("rejectedCaller", "R0202", b"&flag".as_slice()),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.related.span.bytes(), cause, "{name}");
                if code == "R0202" {
                    assert_eq!(finding.site.span.bytes(), b"relay(slot, &flag)");
                }
            }
            for name in ["aliased", "projected"] {
                let pending = lower(name);
                assert_eq!(pending.completion(), Completion::Incomplete, "{name}");
                assert!(pending.mir().is_none());
            }

            let limits = AnalysisLimits {
                max_work: caller.statistics().work,
                max_entries: caller.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(check("caller", limits).mir().is_some());
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
                let limited = check("caller", short);
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
