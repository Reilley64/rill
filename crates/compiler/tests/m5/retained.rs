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
            function storesIncomingReborrow(slot: &mut &bool, src: &mut bool): void {
                *slot = &*src;
            }
            function forwardsIncomingReborrow(slot: &mut &bool, src: &mut bool): void {
                relay(slot, &*src);
            }
            function forwardsLocalReborrow(slot: &mut &bool, input: &bool): void {
                let mut local = true;
                let src = &mut local;
                relay(slot, &*src);
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
            let local = lower("forwardsLocalReborrow");
            assert_eq!(local.completion(), Completion::SourceRejected);
            assert!(local.mir().is_none());
            let [finding] = local.findings() else {
                panic!("expected a local backing escape")
            };
            assert_eq!(finding.code(), "R0202");
            assert_eq!(finding.site.span.bytes(), b"relay(slot, &*src)");
            assert_eq!(finding.related.span.bytes(), b"&mut local");

            for name in ["storesIncomingReborrow", "forwardsIncomingReborrow"] {
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
fn direct_calls_retain_sources_in_named_record_fields() {
    probe(
        "retained::direct_calls_retain_sources_in_named_record_fields",
        r#"
            type Holder = { value: &bool; sibling: &bool; text: string; }
            function remember(slot: &mut &bool, input: &bool): void { *slot = input; }
            function valid(seed: &bool, flag: bool): bool {
                let mut holder = Holder { value: seed, sibling: seed, text: "tag" };
                remember(&mut holder.value, &flag);
                let saved = holder.value;
                return *saved;
            }
            function readSibling(seed: &bool): bool {
                let mut holder = Holder { value: seed, sibling: seed, text: "tag" };
                remember(&mut holder.value, holder.sibling);
                let saved = holder.value;
                return *saved;
            }
            function expires(seed: &bool): bool {
                let mut holder = Holder { value: seed, sibling: seed, text: "tag" };
                { let flag = true; remember(&mut holder.value, &flag); }
                let saved = holder.value;
                return *saved;
            }
            function oldExpires(seed: &bool): bool {
                let mut holder = Holder { value: seed, sibling: seed, text: "tag" };
                { let old = true; holder.value = &old; remember(&mut holder.value, seed); }
                let saved = holder.value;
                return *saved;
            }
            function restored(seed: &bool): bool {
                let mut holder = Holder { value: seed, sibling: seed, text: "tag" };
                { let flag = true; remember(&mut holder.value, &flag); holder.value = seed; }
                let saved = holder.value;
                return *saved;
            }
            function copyExpires(seed: &bool): bool {
                let mut holder = Holder { value: seed, sibling: seed, text: "tag" };
                let mut saved = seed;
                {
                    let flag = true;
                    remember(&mut holder.value, &flag);
                    saved = holder.value;
                    holder.value = seed;
                }
                return *saved;
            }
            function blockedByWhole(seed: &bool): bool {
                let mut holder = Holder { value: seed, sibling: seed, text: "tag" };
                let whole = &holder;
                remember(&mut holder.value, seed);
                let _ = whole;
                return false;
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
            let role = &declaration("remember").id;
            let valid = lower("valid");
            assert!(valid.mir().is_some(), "valid: {:?}", valid.completion());
            for name in ["valid", "readSibling", "restored"] {
                let report = lower(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let ops = linear(mir, mir.entry);
                let field = &field_named(module, "value").id;
                let (access, borrowed) = ops
                    .iter()
                    .enumerate()
                    .find_map(|(at, op)| match op {
                        Operation::Borrow {
                            source,
                            exclusive: true,
                            ..
                        } if source.field().is_some_and(|f| &f.id == field) => Some((at, source)),
                        _ => None,
                    })
                    .expect("exclusive field storage borrow");
                let (invoke, destination, input) = ops
                    .iter()
                    .enumerate()
                    .find_map(|(at, op)| match op {
                        Operation::CallRetaining {
                            callee,
                            destination: RetainedPlace::Local(destination),
                            input,
                        } if *callee == role => Some((at, destination, input)),
                        _ => None,
                    })
                    .expect("retention on the same field");
                assert_eq!(borrowed.root.node, destination.root.node);
                assert_eq!(destination.field().map(|f| &f.id), Some(field));
                assert!(access < invoke);
                let staged = ops[access + 1..invoke]
                    .iter()
                    .find_map(|op| match op {
                        Operation::Copy {
                            from,
                            into: Some(into),
                        } if name == "readSibling"
                            && from
                                .field()
                                .is_some_and(|f| f.id == field_named(module, "sibling").id) =>
                        {
                            Some(into)
                        }
                        Operation::Borrow {
                            holder,
                            exclusive: false,
                            ..
                        } if name != "readSibling" => Some(holder),
                        _ => None,
                    })
                    .expect("stage the shared input after the exclusive field borrow");
                assert_eq!(staged.root.node, input.root.node);
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                    if text.field().is_some_and(|f| f.name.as_deref() == Some("text")))
                );
            }
            for (name, code, related) in [
                ("expires", "R0203", b"&flag".as_slice()),
                ("oldExpires", "R0203", b"&old".as_slice()),
                ("copyExpires", "R0203", b"&flag".as_slice()),
                ("blockedByWhole", "R0201", b"&holder".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected ownership error")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.related.span.bytes(), related);
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
fn exclusive_aliases_retain_sources_in_the_proved_target() {
    probe(
        "retained::exclusive_aliases_retain_sources_in_the_proved_target",
        r#"
            type Holder = { value: &bool; text: string; }
            function remember(slot: &mut &bool, input: &bool): void { *slot = input; }
            function valid(seed: &bool, flag: bool): bool {
                let mut out = seed;
                let access = &mut out;
                let moved = access;
                remember(moved, &flag);
                return *out;
            }
            function field(seed: &bool, flag: bool): bool {
                let mut holder = Holder { value: seed, text: "tag" };
                let access = &mut holder.value;
                remember(access, &flag);
                let saved = holder.value;
                return *saved;
            }
            function expires(seed: &bool): bool {
                let mut out = seed;
                let access = &mut out;
                { let flag = true; remember(access, &flag); }
                return *out;
            }
            function moved(seed: &bool): bool {
                let mut out = seed;
                let access = &mut out;
                let moved = access;
                remember(access, seed);
                return *out;
            }
            function stillExclusive(seed: &bool): bool {
                let mut out = seed;
                let access = &mut out;
                remember(access, seed);
                let value = *out;
                remember(access, seed);
                return value;
            }
            function expiresAtTarget(seed: &bool): bool {
                let mut outer = seed;
                let mut access = &mut outer;
                { let mut inner = seed; access = &mut inner; }
                remember(access, seed);
                return false;
            }
            function expiresThroughAlias(seed: &bool): void {
                let mut out = seed;
                let access = &mut out;
                { let flag = true; remember(access, &flag); }
                remember(access, seed);
            }
            function readBool(value: &mut bool): bool { return *value; }
            function conflictsThroughAlias(seed: &bool): bool {
                let mut out = seed;
                let access = &mut out;
                let mut flag = true;
                remember(access, &flag);
                let value = readBool(&mut flag);
                remember(access, seed);
                return value;
            }
            function choosesTarget(seed: &bool, flag: bool, choose: bool): bool {
                let mut left = seed;
                let mut right = seed;
                let mut access = &mut left;
                if (choose) { access = &mut right; }
                remember(access, &flag);
                return *left;
            }
            function viewOfAlias(seed: &bool): bool {
                let mut out = seed;
                let access = &mut out;
                let mut flag = true;
                remember(access, &flag);
                let view = &access;
                let value = readBool(&mut flag);
                let _ = view;
                return value;
            }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.name.as_deref() == Some(name)
                            && d.id.kind == loader::DeclarationKind::Function
                    })
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let role = &declaration("remember").id;
            let valid = lower("valid");
            for (name, descriptor_name, target_name) in [
                ("valid", b"moved".as_slice(), b"out".as_slice()),
                ("field", b"access".as_slice(), b"holder".as_slice()),
            ] {
                let report = lower(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let events: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::Borrow { .. }
                                | Operation::Reborrow { .. }
                                | Operation::CallRetaining { .. }
                        )
                    })
                    .collect();
                let [
                    Operation::Borrow {
                        source: borrowed,
                        exclusive: true,
                        ..
                    },
                    Operation::Reborrow {
                        parent,
                        exclusive: true,
                        ..
                    },
                    Operation::Borrow {
                        holder: staged,
                        exclusive: false,
                        ..
                    },
                    Operation::CallRetaining {
                        callee,
                        destination: RetainedPlace::ThroughAlias { descriptor, target },
                        input,
                    },
                ] = events.as_slice()
                else {
                    panic!("expected exact borrow, reborrow and retained call: {events:?}")
                };
                assert_eq!(*callee, role);
                assert_eq!(parent.root.node, descriptor.root.node);
                assert_eq!(descriptor.root.span.bytes(), descriptor_name);
                assert_eq!(target.root.span.bytes(), target_name);
                assert_eq!(target.root.node, borrowed.root.node);
                assert_eq!(staged.root.node, input.root.node);
                if name == "field" {
                    let field = &field_named(module, "value").id;
                    assert_eq!(target.field().map(|f| &f.id), Some(field));
                    assert_eq!(borrowed.field().map(|f| &f.id), Some(field));
                    assert!(
                        matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                        if text.field().is_some_and(|f| f.name.as_deref() == Some("text")))
                    );
                } else {
                    assert!(target.projection.is_none());
                }
            }
            for (name, code, cause) in [
                ("expires", "R0203", b"&flag".as_slice()),
                ("moved", "R0200", b"access".as_slice()),
                ("stillExclusive", "R0201", b"&mut out".as_slice()),
                ("expiresAtTarget", "R0203", b"&mut inner".as_slice()),
                ("expiresThroughAlias", "R0203", b"&flag".as_slice()),
                ("conflictsThroughAlias", "R0201", b"&flag".as_slice()),
                ("viewOfAlias", "R0201", b"&flag".as_slice()),
            ] {
                let bad = lower(name);
                assert_eq!(bad.completion(), Completion::SourceRejected, "{name}");
                assert!(bad.mir().is_none());
                let [finding] = bad.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.related.span.bytes(), cause, "{name}");
            }
            let chosen = lower("choosesTarget");
            let mir = chosen
                .mir()
                .expect("retain the exact target on each incoming path");
            let (yes, no) = branch_targets(mir);
            for (edge, expected) in [(yes, b"right".as_slice()), (no, b"left".as_slice())] {
                assert!(linear(mir, edge).iter().any(|op| matches!(op,
                    Operation::CallRetaining { callee, destination: RetainedPlace::ThroughAlias { target, .. }, .. }
                        if *callee == role && target.root.span.bytes() == expected)));
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
fn moved_incoming_access_keeps_the_original_caller_destination() {
    probe(
        "retained::moved_incoming_access_keeps_the_original_caller_destination",
        r#"
            function remember(slot: &mut &bool, input: &bool): void { *slot = input; }
            function relay(slot: &mut &bool, input: &bool): void {
                let access = slot;
                remember(access, input);
                let moved = access;
                remember(moved, input);
            }
            function valid(seed: &bool, flag: bool): bool {
                let mut out = seed;
                relay(&mut out, &flag);
                return *out;
            }
            function chooses(first: &mut &bool, second: &mut &bool, input: &bool, choose: bool): void {
                let mut access = first;
                if (choose) { access = second; }
                remember(access, input);
            }
            function callerExpires(seed: &bool): bool {
                let mut out = seed;
                { let flag = true; relay(&mut out, &flag); }
                return *out;
            }
            function wrongAccess(slot: &mut &bool, input: &bool): void {
                let access = slot;
                remember(slot, input);
            }
            function escape(slot: &mut &bool, input: &bool): void {
                let flag = true;
                let access = slot;
                remember(access, &flag);
            }
            function escapeAfterMove(slot: &mut &bool, input: &bool): void {
                let flag = true;
                *slot = &flag;
                let access = slot;
                remember(access, input);
            }
            function endsBeforeReturn(slot: &mut &bool, input: &bool): void {
                let access = slot;
                { let flag = true; remember(access, &flag); }
            }
            function stillExternal(slot: &mut &bool, input: &bool): void {
                let flag = true;
                { let access = slot; remember(access, &flag); }
            }
            function rejectedCaller(seed: &bool): void {
                let mut out = seed;
                escape(&mut out, seed);
            }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.name.as_deref() == Some(name)
                            && d.id.kind == loader::DeclarationKind::Function
                    })
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let remember = &declaration("remember").id;
            let relay = lower("relay");
            let mir = relay
                .mir()
                .unwrap_or_else(|| panic!("relay: {:?}", relay.completion()));
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::Copy { .. }
                            | Operation::Reborrow { .. }
                            | Operation::CallRetaining { .. }
                    )
                })
                .collect();
            let [
                Operation::Move {
                    from: original,
                    into: Some(access),
                },
                Operation::Reborrow {
                    parent: first_parent,
                    exclusive: true,
                    ..
                },
                Operation::Copy {
                    from: first_source,
                    into: Some(first_staged),
                },
                Operation::CallRetaining {
                    callee: first,
                    destination:
                        RetainedPlace::ThroughParameterAlias {
                            descriptor: first_desc,
                            parameter: first_param,
                        },
                    input: first_input,
                },
                Operation::Move {
                    from,
                    into: Some(moved),
                },
                Operation::Reborrow {
                    parent: next_parent,
                    exclusive: true,
                    ..
                },
                Operation::Copy {
                    from: next_source,
                    into: Some(next_staged),
                },
                Operation::CallRetaining {
                    callee: next,
                    destination:
                        RetainedPlace::ThroughParameterAlias {
                            descriptor: next_desc,
                            parameter: next_param,
                        },
                    input: next_input,
                },
            ] = route.as_slice()
            else {
                panic!("expected moves, reborrows and writes to original parameter: {route:?}")
            };
            assert_eq!(*first, remember);
            assert_eq!(*next, remember);
            assert_eq!(first_source.root.span.bytes(), b"input: &bool");
            assert_eq!(first_source.root.node, next_source.root.node);
            assert_eq!(first_input.root.node, first_staged.root.node);
            assert_eq!(next_input.root.node, next_staged.root.node);
            assert_eq!(original.root.span.bytes(), b"slot: &mut &bool");
            assert_eq!(original.root.node, first_param.root.node);
            assert_eq!(original.root.node, next_param.root.node);
            for place in [first_parent, first_desc, from] {
                assert_eq!(access.root.node, place.root.node);
            }
            for place in [next_parent, next_desc] {
                assert_eq!(moved.root.node, place.root.node);
            }
            let chosen = lower("chooses");
            let mir = chosen
                .mir()
                .expect("the exact destination is preserved on each path");
            let (yes, no) = branch_targets(mir);
            for (edge, expected) in [
                (yes, b"second: &mut &bool".as_slice()),
                (no, b"first: &mut &bool".as_slice()),
            ] {
                let calls: Vec<_> = linear(mir, edge)
                    .into_iter()
                    .filter(|op| matches!(op, Operation::CallRetaining { .. }))
                    .collect();
                assert!(matches!(calls.as_slice(), [
                    Operation::CallRetaining { callee, destination: RetainedPlace::ThroughParameterAlias { parameter, descriptor }, .. }
                ] if *callee == remember && parameter.root.span.bytes() == expected && descriptor.root.span.bytes() == b"access"));
            }
            let valid = lower("valid");
            let mir = valid
                .mir()
                .expect("the owner and both the relay and its reached callee check");
            assert!(linear(mir, mir.entry).iter().any(|op| matches!(op,
                Operation::CallRetaining { callee, destination: RetainedPlace::Local(place), .. }
                if *callee == &declaration("relay").id && place.root.span.bytes() == b"out")));

            for (name, code, related) in [
                ("callerExpires", "R0203", b"&flag".as_slice()),
                ("wrongAccess", "R0200", b"slot".as_slice()),
                ("escape", "R0202", b"&flag".as_slice()),
                ("escapeAfterMove", "R0202", b"&flag".as_slice()),
                ("endsBeforeReturn", "R0203", b"&flag".as_slice()),
                ("stillExternal", "R0202", b"&flag".as_slice()),
                ("rejectedCaller", "R0202", b"&flag".as_slice()),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected ownership diagnostic")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.related.span.bytes(), related);
                if code == "R0202" {
                    let store = if name == "escapeAfterMove" {
                        b"remember(access, input)".as_slice()
                    } else {
                        b"remember(access, &flag)"
                    };
                    assert_eq!(finding.site.span.bytes(), store);
                }
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
fn exact_alias_reads_and_writes_keep_value_and_access_separate() {
    probe(
        "retained::exact_alias_reads_and_writes_keep_value_and_access_separate",
        r#"
            function readIncoming(slot: &mut &bool): &bool {
                let access = slot;
                return *access;
            }
            function replace(seed: &bool): bool {
                let mut out = seed;
                let mut other = seed;
                let mut access = &mut other;
                {
                    let flag = true;
                    out = &flag;
                    access = &mut out;
                    *access = seed;
                }
                let result = *access;
                return *result;
            }
            function copyBeforeReplacement(seed: &bool): bool {
                let mut out = seed;
                let access = &mut out;
                let mut saved = seed;
                { let flag = true; *access = &flag; saved = *access; *access = seed; }
                return *saved;
            }
            type Holder = { value: &bool; sibling: &bool; }
            function futureFieldCopy(seed: &bool): bool {
                let mut holder = Holder { value: seed, sibling: seed };
                let access = &mut holder.value;
                { let flag = true; holder.sibling = &flag; }
                let value = *access;
                return *value;
            }
            function copiedFieldEscape(seed: &bool): &bool {
                let flag = true;
                let mut holder = Holder { value: seed, sibling: &flag };
                let access = &mut holder.value;
                return *access;
            }
            function withWrite(value: &mut bool, seed: &bool): &bool { return seed; }
            function beforeCommit(seed: &bool): void {
                let mut flag = true;
                let mut out = &flag;
                let access = &mut out;
                *access = withWrite(&mut flag, seed);
            }
            function remember(slot: &mut &bool, input: &bool): void { *slot = input; }
            function suspended(seed: &bool): void {
                let mut out = seed;
                let access = &mut out;
                remember(access, *access);
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
            let incoming = lower("readIncoming");
            let mir = incoming
                .mir()
                .unwrap_or_else(|| panic!("readIncoming: {:?}", incoming.completion()));
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| matches!(op, Operation::CopyThrough { .. } | Operation::Export(_)))
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::CopyThrough { descriptor, into: Some(copied) },
                Operation::Export(exported),
            ] if descriptor.root.span.bytes() == b"access" && copied.root.node == exported.root.node));

            let replaced = lower("replace");
            let mir = replaced
                .mir()
                .expect("an exact overwrite retires the old backing from live access");
            let ops = linear(mir, mir.entry);
            let route: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::InstallThrough { .. }
                            | Operation::CopyThrough { .. }
                            | Operation::Dereference(_)
                    )
                })
                .collect();
            let [
                Operation::InstallThrough {
                    descriptor: write,
                    incoming,
                },
                Operation::CopyThrough {
                    descriptor: read,
                    into: Some(copied),
                },
                Operation::Dereference(used),
            ] = route.as_slice()
            else {
                panic!("expected alias write, copy and use: {route:?}");
            };
            assert_eq!(write.root.span.bytes(), b"access");
            assert_eq!(write.root.node, read.root.node);
            assert_eq!(copied.root.node, used.root.node);
            let (before_write, _) = ops.split_at(
                ops.iter()
                    .position(|op| matches!(op, Operation::InstallThrough { .. }))
                    .unwrap(),
            );
            let transfers: Vec<_> = before_write
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow {
                            exclusive: true,
                            ..
                        } | Operation::Install { .. }
                            | Operation::Copy { .. }
                    )
                })
                .collect();
            assert!(matches!(transfers.as_slice(), [..,
                Operation::Borrow { source, holder, exclusive: true },
                Operation::Install { target, incoming: access },
                Operation::Copy { from, into: Some(staged) },
            ] if source.root.span.bytes() == b"out" && holder.root.node == access.root.node && target.root.node == write.root.node
                && from.root.span.bytes() == b"seed: &bool" && staged.root.node == incoming.root.node));

            let limits = AnalysisLimits {
                max_work: replaced.statistics().work,
                max_entries: replaced.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("replace", limits).completion(), Completion::Checked);
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
                let short = check("replace", short);
                assert_eq!(short.completion(), Completion::ResourceLimit);
                assert!(short.mir().is_none());
            }
            for (name, code, site, related) in [
                (
                    "futureFieldCopy",
                    "R0203",
                    b"{ let flag = true; holder.sibling = &flag; }".as_slice(),
                    b"&flag".as_slice(),
                ),
                (
                    "copiedFieldEscape",
                    "R0202",
                    b"return *access;".as_slice(),
                    b"flag".as_slice(),
                ),
                (
                    "beforeCommit",
                    "R0201",
                    b"&mut flag".as_slice(),
                    b"&flag".as_slice(),
                ),
                (
                    "copyBeforeReplacement",
                    "R0203",
                    b"{ let flag = true; *access = &flag; saved = *access; *access = seed; }"
                        .as_slice(),
                    b"&flag".as_slice(),
                ),
                (
                    "suspended",
                    "R0201",
                    b"*access".as_slice(),
                    b"access".as_slice(),
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected ownership diagnostic")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(
                    std::str::from_utf8(finding.site.span.bytes()),
                    std::str::from_utf8(site),
                    "{name} primary"
                );
                assert_eq!(
                    std::str::from_utf8(finding.related.span.bytes()),
                    std::str::from_utf8(related),
                    "{name} related"
                );
            }
        },
    );
}

#[test]
fn moved_incoming_write_checks_the_original_caller_contents() {
    probe(
        "retained::moved_incoming_write_checks_the_original_caller_contents",
        r#"
            function overwrite(slot: &mut &bool, seed: &bool): &bool {
                let access = slot;
                {
                    let flag = true;
                    *access = &flag;
                    *access = seed;
                }
                return *access;
            }
            function escapes(slot: &mut &bool): void {
                let flag = true;
                { let access = slot; *access = &flag; }
            }
            function expires(slot: &mut &bool, seed: &bool): void {
                let access = slot;
                { let flag = true; *access = &flag; }
                *access = seed;
            }
            function originalMoved(slot: &mut &bool, seed: &bool): void {
                let access = slot;
                *slot = seed;
                *access = seed;
            }
            function touch(place: &mut &mut &bool): void {}
            function forgetOrigin(slot: &mut &bool): void {
                let mut access = slot;
                touch(&mut access);
            }
        "#,
        |_, module, receipt| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let report = lower("overwrite");
            let mir = report
                .mir()
                .expect("the overwrite retires the caller contents' local backing");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::Borrow { .. }
                            | Operation::Copy { .. }
                            | Operation::InstallThrough { .. }
                            | Operation::CopyThrough { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Move { from: original, into: Some(access) },
                Operation::Borrow { source: flag, holder, exclusive: false },
                Operation::InstallThrough { descriptor: first, incoming: borrowed },
                Operation::Copy { from: seed, into: Some(staged) },
                Operation::InstallThrough { descriptor: second, incoming },
                Operation::CopyThrough { descriptor: read, into: Some(copied) },
                Operation::Export(exported),
            ] if original.root.span.bytes() == b"slot: &mut &bool" && access.root.span.bytes() == b"access"
                && flag.root.span.bytes() == b"flag" && seed.root.span.bytes() == b"seed: &bool"
                && [first, second, read].iter().all(|place| place.root.node == access.root.node)
                && borrowed.root.node == holder.root.node && incoming.root.node == staged.root.node
                && copied.root.node == exported.root.node));
            for (name, code, site, related) in [
                ("escapes", "R0202", "*access = &flag;", "&flag"),
                (
                    "expires",
                    "R0203",
                    "{ let flag = true; *access = &flag; }",
                    "&flag",
                ),
                ("originalMoved", "R0200", "*slot = seed;", "slot"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none(), "{name}");
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
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
            function captureMoved(slot: &mut &bool, input: &bool): mut () => void {
                let access = slot;
                return () => { *access = input; };
            }
            function forward(slot: &mut &bool): &mut &bool { return slot; }
            function callProduced(slot: &mut &bool, input: &bool): void {
                let access = forward(slot);
                *access = (input);
            }
            function joined(slot: &mut &bool, other: &mut &bool, input: &bool, choose: bool): void {
                let mut access = slot;
                if (choose) { access = forward(other); }
                *access = ((input));
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
            for store in [
                b"*slot = input;".as_slice(),
                b"*access = input;",
                b"*access = (input);",
                b"*access = ((input));",
                b"*slot = incoming;",
            ] {
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

fn field_named<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.id.kind == loader::DeclarationKind::Field && d.name.as_deref() == Some(name))
        .unwrap()
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
