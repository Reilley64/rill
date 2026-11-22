use super::*;

#[path = "disposers/mixed_wrapper.rs"]
mod mixed_wrapper;

#[test]
fn custom_record_rebuild() {
    use compiler::analysis::{DeclarationShape, ownership::Place, scalars::NodeFactKind};
    use compiler::parser::NodeKind;

    let mut failures = Vec::new();
    probe_with_typed(
        "disposers::custom_record_rebuild",
        r#"
            import std::core::dispose as release;
            type Custom = { keep: string; mark: bool; last: string; }
            implement release(value: Custom): void { dispose(value.last); }
            function select(value: Custom): void { release(value); }
            function rebuilt(base: Custom, replacement: string, before: string, after: string): void {
                {
                    let saved = Custom { ...base, last: replacement, mark: base.mark };
                    dispose(before);
                }
                dispose(after);
            }
            function reused(base: Custom, replacement: string, before: string, after: string): void {
                {
                    let saved = Custom { ...base, last: replacement, mark: base.mark };
                    let taken = base.keep;
                    dispose(before);
                }
                dispose(after);
            }
            type Escape = { text: string; }
            implement release(value: Escape): void {
                let rebuilt = Escape { ...value, text: "new" };
            }
            function activeEscape(value: Escape): void {}
            type Plain = { keep: string; mark: bool; last: string; }
            function repair(base: Plain, replacement: string): void {
                let taken = base.last;
                let saved = Plain { ...base, last: replacement, mark: base.mark };
            }
        "#,
        |request, module, receipt, typed| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let alias_call = module
                .syntax
                .nodes
                .iter()
                .position(|node| {
                    node.kind == NodeKind::Call && node.span.bytes() == b"release(value)"
                })
                .unwrap();
            let NodeFactKind::Expression(alias) = &typed
                .bodies()
                .node(module, alias_call)
                .unwrap()
                .unwrap()
                .kind
            else {
                panic!("expected the typed alias call");
            };
            let alias = alias.call.as_deref().unwrap();
            let canonical = &typed.dispose_role().unwrap().id;
            assert_eq!(alias.callee, canonical);
            let selected = alias.selection.as_ref().unwrap().implementation;

            let record = typed
                .inventory()
                .type_facts()
                .declarations()
                .find(|facts| {
                    facts.declaration.id.module == module.key
                        && facts.declaration.name.as_deref() == Some("Custom")
                })
                .unwrap();
            let DeclarationShape::Record { fields } = &record.shape else {
                panic!("expected the checked Custom fields");
            };
            let field = |name| {
                &fields
                    .iter()
                    .find(|member| member.declaration.name.as_deref() == Some(name))
                    .unwrap()
                    .declaration
                    .id
            };
            let (keep, mark, last) = (field("keep"), field("mark"), field("last"));
            let is_root = |place: &Place<'_>, bytes: &[u8]| {
                place.root.module == &module.key
                    && place.root.span.bytes() == bytes
                    && place.projection.is_none()
            };
            let same_root = |a: &Place<'_>, b: &Place<'_>| {
                a.root.module == b.root.module && a.root.node == b.root.node
            };
            let check_disposer = |mir: &Mir<'_>| {
                let active = mir
                    .disposer(selected)
                    .expect("selected custom disposer body");
                let effects = rebuild_effects(active);
                let [Operation::Dispose { callee, operand }, _] = effects.as_slice() else {
                    panic!("expected explicit disposal followed by remaining-field cleanup");
                };
                assert_eq!(*callee, canonical);
                assert_eq!(operand.root.span.bytes(), b"value: Custom");
                assert_eq!(operand.field().map(|f| &f.id), Some(last));
                assert!(
                    matches!(path_drops(active, active.entry).as_slice(), [Obligation::Drop(place)]
                    if place.field().is_some_and(|f| &f.id == keep))
                );
            };
            let control = lower("select");
            let control = control
                .mir()
                .expect("ordinary canonical custom disposal must lower");
            assert!(matches!(path_drops(control, control.entry).as_slice(),
                [Obligation::InvokeDisposer { operand, implementation }]
                    if is_root(operand, b"value: Custom") && *implementation == selected));
            check_disposer(control);
            let active = lower("activeEscape");
            assert_eq!(active.completion(), Completion::SourceRejected);
            assert!(active.mir().is_none());
            let [finding] = active.findings() else {
                panic!("expected one active-disposer escape finding")
            };
            assert_eq!(finding.code(), "R0501");
            assert_eq!(
                finding.site.span.bytes(),
                b"Escape { ...value, text: \"new\" }"
            );

            // A partially moved plain base avoids the separate restriction on moving
            // a field out of an ordinary custom owner. Repair is not a checked slice.
            let repair = lower("repair");
            assert_eq!(repair.completion(), Completion::Incomplete);
            assert!(repair.findings().is_empty() && repair.mir().is_none());

            for (name, expected) in [
                ("rebuilt", Completion::Checked),
                ("reused", Completion::SourceRejected),
            ] {
                let report = lower(name);
                if report.completion() != expected {
                    failures.push(format!(
                        "test_mode={}, {name}: M4 receipt issued; expected {expected:?}, got {:?} at {:?}; MIR={}, findings={}",
                        request.test_mode,
                        report.completion(),
                        report.blocked_at().map(|site| site.span.bytes().escape_ascii().to_string()),
                        report.mir().is_some(),
                        report.findings().len(),
                    ));
                    continue;
                }
                if name == "reused" {
                    assert!(report.mir().is_none());
                    let [finding] = report.findings() else {
                        panic!("expected one moved-field finding");
                    };
                    assert_eq!(finding.code(), "R0200");
                    assert_eq!(finding.site.span.bytes(), b"base.keep");
                    continue;
                }

                let mir = report.mir().expect("rebuild MIR");
                let effects = rebuild_effects(mir);
                // A whole-region handoff carries keep; there is no separate field
                // read, initialization, or cleanup of keep on this path.
                let [
                    Operation::Move {
                        from: replacement,
                        into: Some(last_stage),
                    },
                    Operation::Copy {
                        from: original_mark,
                        into: Some(mark_stage),
                    },
                    Operation::Drop(old),
                    Operation::Install {
                        target: last_target,
                        incoming: new_last,
                    },
                    Operation::Install {
                        target: mark_target,
                        incoming: new_mark,
                    },
                    Operation::Move {
                        from: base,
                        into: Some(saved),
                    },
                    Operation::Dispose {
                        operand: before, ..
                    },
                    Operation::Cleanup(exit),
                    Operation::Dispose { operand: after, .. },
                ] = effects.as_slice()
                else {
                    panic!(
                        "expected written-order staging, displacement, handoff, then scope exit"
                    );
                };
                assert!(is_root(replacement, b"replacement: string"));
                assert!(is_root(base, b"base: Custom") && is_root(saved, b"saved"));
                assert!(!same_root(last_stage, mark_stage));
                for stage in [last_stage, mark_stage] {
                    assert!(stage.projection.is_none());
                    assert!(
                        [replacement, base, saved]
                            .iter()
                            .all(|root| !same_root(stage, root))
                    );
                }
                for (place, id) in [
                    (original_mark, mark),
                    (last_target, last),
                    (mark_target, mark),
                ] {
                    assert!(same_root(place, base));
                    assert_eq!(place.field().map(|f| &f.id), Some(id));
                }
                assert!(new_last.projection.is_none() && same_root(new_last, last_stage));
                assert!(new_mark.projection.is_none() && same_root(new_mark, mark_stage));
                assert!(is_root(before, b"before: string") && is_root(after, b"after: string"));

                assert!(same_root(old, base) && old.field().is_some_and(|f| &f.id == last));
                let [
                    Obligation::InvokeDisposer {
                        operand,
                        implementation,
                    },
                ] = exit.as_slice()
                else {
                    panic!("expected only the rebuilt owner's disposer at lexical exit");
                };
                assert!(same_root(operand, saved) && operand.projection.is_none());
                assert_eq!(*implementation, selected);
                check_disposer(mir);
            }
        },
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn rebuild_effects<'a, 'g>(mir: &'a Mir<'g>) -> Vec<&'a Operation<'g>> {
    linear(mir, mir.entry)
        .into_iter()
        .filter(|op| match op {
            Operation::Move { .. }
            | Operation::Copy { .. }
            | Operation::Install { .. }
            | Operation::Dispose { .. }
            | Operation::Drop(_) => true,
            Operation::Initialize(place) => place.projection.is_some(),
            Operation::Cleanup(actions) => actions.iter().any(|action| {
                matches!(
                    action,
                    Obligation::Drop(_) | Obligation::InvokeDisposer { .. }
                )
            }),
            _ => false,
        })
        .collect()
}

#[test]
fn protected_guard_cleanup_follows_the_initialized_owner() {
    probe_with_typed(
        "disposers::protected_guard_cleanup_follows_the_initialized_owner",
        r#"
            import std::mutex::LockGuard as Ticket;
            function paths(lock: Ticket<i32>, owner: string, explicit: bool): void {
                let held = lock;
                if (explicit) { dispose((held)); return; }
            }
            function replace(first: LockGuard<i32>, second: LockGuard<i32>): void {
                let mut held = first;
                held = second;
            }
            function replaceMoved(first: LockGuard<i32>, second: LockGuard<i32>): void {
                let mut held = first;
                dispose(held);
                held = second;
            }
            function selfReplace(lock: LockGuard<i32>): void {
                let mut held = lock;
                held = held;
            }
            function loopExit(lock: LockGuard<i32>, enter: bool): void {
                while (enter) { let held = lock; break; }
            }
            function rejects(lock: LockGuard<i32>): void {
                let held = lock;
                dispose(held);
                dispose(held);
            }
            function borrowed(lock: LockGuard<i32>): void { let view = &lock; }
            function borrowedRelay(owned: LockGuard<i32>, view: &LockGuard<i32>): &LockGuard<i32> {
                return view;
            }
            function borrowedRelayOther(owned: LockGuard<i32>, view: &LockGuard<bool>): &LockGuard<bool> {
                return view;
            }
            function borrowedOnly(view: &LockGuard<i32>): &LockGuard<i32> { return view; }
            function passes(lock: LockGuard<i32>): LockGuard<i32> { return lock; }
            function takes(lock: LockGuard<i32>): void { }
            function callArgument(lock: LockGuard<i32>): void { takes(lock); }
            function skippedCall(lock: LockGuard<i32>): void { if (false) { takes(lock); } }
            function callOutput(lock: LockGuard<i32>): void { let held = passes(lock); }
            function other(lock: LockGuard<bool>): void { }
        "#,
        |_, module, receipt, typed| {
            use compiler::analysis::scalars::{NodeFactKind, data::DataExpression};
            use compiler::analysis::typed::{
                NativeOwnershipAction, OwnershipCoverage, OwnershipRule,
            };

            let check = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let lower = |name| check(name, AnalysisLimits::default());
            let scope = typed
                .ownership_protection()
                .iter()
                .find(|scope| {
                    scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                        && scope.coverage == OwnershipCoverage::Clear
                })
                .unwrap();
            let provider = scope.provider.as_ref().unwrap();
            let report = lower("paths");
            let mir = report.mir().expect("received guard has normal cleanup");
            let (yes, no) = branch_targets(mir);
            let transfers: Vec<_> = mir
                .blocks
                .iter()
                .filter_map(|block| match &block.operation {
                    Operation::Move {
                        from,
                        into: Some(to),
                    } => Some((from, to)),
                    _ => None,
                })
                .collect();
            let [(input, held)] = transfers.as_slice() else {
                panic!("expected one transfer of the guard")
            };
            assert_eq!(input.root.span.bytes(), b"lock: Ticket<i32>");
            assert_eq!(held.root.span.bytes(), b"held");
            for edge in [yes, no] {
                assert!(matches!(path_drops(mir, edge).as_slice(), [
                    Obligation::ReleaseGuard { operand, implementation, entry },
                    Obligation::Drop(owner),
                ] if operand.root.node == held.root.node && *implementation == &provider.implementation.id
                    && *entry == &provider.entry.id && owner.root.span.bytes() == b"owner: string"));
            }
            let releases: Vec<_> = mir.blocks.iter().filter(|block| matches!(&block.operation,
                Operation::Cleanup(cleanup) if cleanup.iter().any(|action| matches!(action, Obligation::ReleaseGuard { .. }))
            )).collect();
            assert_eq!(releases.len(), 2);
            let explicit = releases
                .into_iter()
                .find(|b| b.site.span.bytes() == b"dispose((held))")
                .unwrap();
            let fact = typed
                .bodies()
                .node(module, explicit.site.node)
                .unwrap()
                .unwrap();
            let NodeFactKind::Expression(facts) = &fact.kind else {
                panic!("expected typed call")
            };
            let Some(DataExpression::NativeOwnership(native)) = &facts.data else {
                panic!("expected protected release fact")
            };
            assert_eq!(native.action, NativeOwnershipAction::GuardRelease);
            assert_eq!(native.implementation, &provider.implementation.id);
            assert_eq!(native.entry, &provider.entry.id);

            let replace = lower("replace");
            let mir = replace
                .mir()
                .expect("replacement releases the old initialized owner");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Move { .. } | Operation::Install { .. } => true,
                    Operation::Cleanup(cleanups) => cleanups
                        .iter()
                        .any(|action| matches!(action, Obligation::ReleaseGuard { .. })),
                    _ => false,
                })
                .collect();
            let [
                Operation::Move {
                    from: first,
                    into: Some(held),
                },
                Operation::Move {
                    from: second,
                    into: Some(staged),
                },
                Operation::Cleanup(displaced),
                Operation::Install { target, incoming },
                Operation::Cleanup(final_cleanup),
            ] = route.as_slice()
            else {
                panic!("expected staging, release, install, and one final release")
            };
            assert_eq!(first.root.span.bytes(), b"first: LockGuard<i32>");
            assert_eq!(second.root.span.bytes(), b"second: LockGuard<i32>");
            assert_eq!(target.root.node, held.root.node);
            assert_eq!(staged.root.node, incoming.root.node);
            for actions in [displaced, final_cleanup] {
                assert!(
                    matches!(actions.as_slice(), [Obligation::ReleaseGuard { operand, implementation, entry }]
                    if operand.root.node == held.root.node && *implementation == &provider.implementation.id
                        && *entry == &provider.entry.id)
                );
            }

            let moved = lower("replaceMoved");
            let mir = moved
                .mir()
                .expect("installing in a moved slot needs no displacement release");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Move { .. } | Operation::Install { .. } => true,
                    Operation::Cleanup(cleanups) => cleanups
                        .iter()
                        .any(|action| matches!(action, Obligation::ReleaseGuard { .. })),
                    _ => false,
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Move { into: Some(held), .. },
                Operation::Cleanup(first),
                Operation::Move { into: Some(staged), .. },
                Operation::Install { target, incoming },
                Operation::Cleanup(second),
            ] if target.root.node == held.root.node && incoming.root.node == staged.root.node
                && [first, second].iter().all(|actions| matches!(actions.as_slice(), [
                    Obligation::ReleaseGuard { operand, implementation, entry }
                ] if operand.root.node == held.root.node && *implementation == &provider.implementation.id
                    && *entry == &provider.entry.id))));

            let self_replace = lower("selfReplace");
            let mir = self_replace
                .mir()
                .expect("self-replacement stages ownership without dropping it");
            assert!(matches!(path_drops(mir, mir.entry).as_slice(), [
                Obligation::ReleaseGuard { operand, implementation, entry }
            ] if operand.root.span.bytes() == b"held" && *implementation == &provider.implementation.id
                && *entry == &provider.entry.id));
            let loop_exit = lower("loopExit");
            let mir = loop_exit
                .mir()
                .expect("loop exit cleans its initialized path owner");
            let (yes, no) = branch_targets(mir);
            for (edge, owner) in [
                (yes, b"held".as_slice()),
                (no, b"lock: LockGuard<i32>".as_slice()),
            ] {
                assert!(matches!(path_drops(mir, edge).as_slice(), [
                    Obligation::ReleaseGuard { operand, implementation, entry }
                ] if operand.root.span.bytes() == owner && *implementation == &provider.implementation.id
                    && *entry == &provider.entry.id));
            }
            let skipped = lower("skippedCall");
            let mir = skipped
                .mir()
                .expect("the typed but unsupported call is infeasible");
            assert!(
                !linear(mir, mir.entry)
                    .iter()
                    .any(|op| matches!(op, Operation::Call(_)))
            );
            assert!(matches!(path_drops(mir, mir.entry).as_slice(), [
                Obligation::ReleaseGuard { operand, .. }
            ] if operand.root.span.bytes() == b"lock: LockGuard<i32>"));

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
                let limited = check("paths", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            let rejected = lower("rejects");
            assert_eq!(rejected.completion(), Completion::SourceRejected);
            assert!(rejected.mir().is_none());
            let [finding] = rejected.findings() else {
                panic!("expected one moved-guard finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"dispose(held)");
            assert_eq!(finding.related.span.bytes(), b"dispose(held)");
            assert!(finding.related.span.start() < finding.site.span.start());
            let borrowed = lower("borrowedOnly");
            let mir = borrowed
                .mir()
                .expect("ordinary incoming references remain forwardable");
            assert!(path_drops(mir, mir.entry).is_empty());
            for (name, expected) in [
                ("borrowed", "&lock"),
                ("borrowedRelay", "view: &LockGuard<i32>"),
                ("borrowedRelayOther", "view: &LockGuard<bool>"),
                ("passes", "lock"),
                ("callArgument", "lock"),
                ("callOutput", "passes(lock)"),
                ("other", "lock: LockGuard<bool>"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    std::str::from_utf8(report.blocked_at().unwrap().span.bytes()),
                    Ok(expected),
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn protected_fields_keep_their_obligation_through_disposer_cleanup() {
    probe_with_typed(
        "disposers::protected_fields_keep_their_obligation_through_disposer_cleanup",
        r#"
            import std::core::dispose as release;
            type Plain = { plainHead: string; plainLock: LockGuard<i32>; plainTail: string; }
            function ordinary(value: Plain): void { }
            function partial(value: Plain): void { let held = value.plainLock; dispose(held); }
            function duplicateRelease(value: Plain): void { dispose(value.plainLock); dispose(value.plainLock); }
            function borrowedWrapper(value: Plain): void { let view = &value; }
            function mixedWrapper(owned: Plain, view: &Plain): &Plain { return view; }
            function wrapperReferenceOnly(view: &Plain): &Plain { return view; }
            function pass(value: Plain): Plain { return value; }
            function callArgument(value: Plain): void { ordinary(value); }
            function callOutput(value: Plain): void { let _ = pass(value); }
            function replaceField(value: Plain, next: LockGuard<i32>): void {
                let mut active = value;
                active.plainLock = next;
            }
            type Other = { unsupported: LockGuard<bool>; }
            function otherType(value: Other): void { }
            type Empty = { emptyHead: string; emptyLock: LockGuard<i32>; emptyTail: string; }
            implement release(value: Empty): void { }
            function empty(value: Empty): void { release(value); }
            function outside(value: Empty): void { let held = value.emptyLock; }
            function outsideExplicit(value: Empty): void { dispose(value.emptyLock); }
            type Branch = { head: string; lock: LockGuard<i32>; tail: string; take: bool; }
            implement release(value: Branch): void {
                {
                    let active = value;
                    if (active.take) { let held = active.lock; dispose(held); }
                }
                let marker = "marker";
                dispose(marker);
            }
            function branch(value: Branch): void { }
            type Two = { firstLock: LockGuard<i32>; secondLock: LockGuard<i32>; }
            implement release(value: Two): void { dispose(value.firstLock); }
            function two(value: Two): void { }
        "#,
        |_, module, receipt, typed| {
            use compiler::analysis::typed::{
                NativeOwnershipAction, OwnershipCoverage, OwnershipRule,
            };

            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let field = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == loader::DeclarationKind::Field
                            && d.name.as_deref() == Some(name)
                    })
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let provider = typed
                .ownership_protection()
                .iter()
                .find(|scope| {
                    scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                        && scope.coverage == OwnershipCoverage::Clear
                })
                .unwrap()
                .provider
                .as_ref()
                .unwrap();
            let assert_explicit = |block: &compiler::analysis::ownership::Block<'_>,
                                   expected: &[u8]| {
                use compiler::analysis::scalars::{NodeFactKind, data::DataExpression};

                assert_eq!(block.site.span.bytes(), expected);
                let fact = typed
                    .bodies()
                    .node(module, block.site.node)
                    .unwrap()
                    .unwrap();
                let NodeFactKind::Expression(facts) = &fact.kind else {
                    panic!("expected typed release call")
                };
                let Some(DataExpression::NativeOwnership(native)) = &facts.data else {
                    panic!("expected selected native release")
                };
                assert_eq!(native.action, NativeOwnershipAction::GuardRelease);
                assert_eq!(native.implementation, &provider.implementation.id);
                assert_eq!(native.entry, &provider.entry.id);
            };
            let is_release = |obligation| match obligation {
                Obligation::ReleaseGuard {
                    operand,
                    implementation,
                    entry,
                } => {
                    assert_eq!(implementation, &provider.implementation.id);
                    assert_eq!(entry, &provider.entry.id);
                    Some(operand)
                }
                _ => None,
            };
            let ordinary = lower("ordinary");
            let mir = ordinary
                .mir()
                .unwrap_or_else(|| panic!("ordinary: {:?}", ordinary.completion()));
            let drops = path_drops(mir, mir.entry);
            let [Obligation::Drop(tail), guard, Obligation::Drop(head)] = drops.as_slice() else {
                panic!("expected ordinary reverse field cleanup: {drops:?}");
            };
            let lock = is_release(*guard).unwrap();
            for (place, expected) in [
                (tail, "plainTail"),
                (&lock, "plainLock"),
                (head, "plainHead"),
            ] {
                assert_eq!(place.field().unwrap().id, field(expected).id);
                assert_eq!(place.root.span.bytes(), b"value: Plain");
            }
            let partial = lower("partial");
            let mir = partial.mir().expect("the moved guard has its own cleanup");
            let blocks = straight_blocks(mir, mir.entry);
            assert!(matches!(
                blocks.last().unwrap().terminator,
                Terminator::Return
            ));
            let transfers: Vec<_> = blocks
                .iter()
                .filter_map(|block| match &block.operation {
                    Operation::Move {
                        from,
                        into: Some(to),
                    } => Some((from, to)),
                    _ => None,
                })
                .collect();
            let [(from, held)] = transfers.as_slice() else {
                panic!("expected a single field transfer")
            };
            assert_eq!(from.field().unwrap().id, field("plainLock").id);
            let drops = path_drops(mir, mir.entry);
            let [guard, Obligation::Drop(tail), Obligation::Drop(head)] = drops.as_slice() else {
                panic!("the wrapper must not repeat release of the moved field");
            };
            assert_eq!(is_release(*guard).unwrap().root.node, held.root.node);
            let explicit = blocks.iter().find(|block| matches!(&block.operation,
                Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action, Obligation::ReleaseGuard { .. }))
            )).unwrap();
            assert_explicit(explicit, b"dispose(held)");
            assert_eq!(tail.field().unwrap().id, field("plainTail").id);
            assert_eq!(head.field().unwrap().id, field("plainHead").id);

            let empty = lower("empty");
            let mir = empty
                .mir()
                .expect("an empty custom disposer cannot suppress protected cleanup");
            let drops = path_drops(mir, mir.entry);
            let [
                Obligation::InvokeDisposer {
                    operand,
                    implementation,
                },
            ] = drops.as_slice()
            else {
                panic!("the caller transfers the entire wrapper exactly once");
            };
            assert_eq!(operand.root.span.bytes(), b"value: Empty");
            let disposer = mir.disposer(implementation).unwrap();
            let drops = path_drops(disposer, disposer.entry);
            let [Obligation::Drop(tail), guard, Obligation::Drop(head)] = drops.as_slice() else {
                panic!("empty body's epilogue cleans the remaining fields");
            };
            let lock = is_release(*guard).unwrap();
            for (place, expected) in [
                (tail, "emptyTail"),
                (&lock, "emptyLock"),
                (head, "emptyHead"),
            ] {
                assert_eq!(place.field().unwrap().id, field(expected).id);
            }

            let branch = lower("branch");
            let mir = branch
                .mir()
                .expect("checked disposer transfers cleanup into its epilogue");
            let drops = path_drops(mir, mir.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = drops.as_slice() else {
                panic!("expected one outer disposer invocation");
            };
            let body = mir.disposer(implementation).unwrap();
            let prefix = straight_blocks(body, body.entry);
            let moves: Vec<_> = prefix
                .iter()
                .filter_map(|block| match &block.operation {
                    Operation::Move {
                        from,
                        into: Some(to),
                    } => Some((from, to)),
                    _ => None,
                })
                .collect();
            let [(owned, active)] = moves.as_slice() else {
                panic!("expected one move of the active disposer operand");
            };
            assert_eq!(owned.root.span.bytes(), b"value: Branch");
            assert_eq!(active.root.span.bytes(), b"active");
            let Terminator::Branch { yes, no } = prefix.last().unwrap().terminator else {
                panic!("expected a disposer branch");
            };
            for (edge, moved) in [(yes, true), (no, false)] {
                let mut events = Vec::new();
                let mut transferred = None;
                let mut deferred = None;
                let blocks = straight_blocks(body, edge);
                assert!(matches!(
                    blocks.last().unwrap().terminator,
                    Terminator::Return
                ));
                for block in blocks {
                    match &block.operation {
                        Operation::Move {
                            from,
                            into: Some(to),
                        } => {
                            assert_eq!(from.field().unwrap().id, field("lock").id);
                            assert_eq!(from.root.node, active.root.node);
                            transferred = Some(to.root.node);
                            events.push("move");
                        }
                        Operation::Dispose { operand, .. } => {
                            assert_eq!(operand.root.span.bytes(), b"marker");
                            events.push("marker");
                        }
                        Operation::Cleanup(cleanup) => {
                            for obligation in cleanup {
                                match obligation {
                                    Obligation::DeferDisposerFields { from, into } => {
                                        assert_eq!(from.root.node, active.root.node);
                                        deferred = Some(into.root.node);
                                        events.push("defer");
                                    }
                                    Obligation::ReleaseGuard { .. } => {
                                        let operand = is_release(*obligation).unwrap();
                                        if moved {
                                            assert_explicit(block, b"dispose(held)");
                                            assert!(operand.field().is_none());
                                            assert_eq!(Some(operand.root.node), transferred);
                                        } else {
                                            assert_eq!(
                                                operand.field().unwrap().id,
                                                field("lock").id
                                            );
                                            assert_eq!(Some(operand.root.node), deferred);
                                        }
                                        events.push("release");
                                    }
                                    Obligation::Drop(place) => {
                                        assert_eq!(Some(place.root.node), deferred);
                                        let member = place.field().unwrap();
                                        if member.id == field("tail").id {
                                            events.push("tail");
                                        } else {
                                            assert_eq!(member.id, field("head").id);
                                            events.push("head");
                                        }
                                    }
                                    Obligation::EndStorage(_) => {}
                                    _ => panic!("the active outer disposer cannot run again"),
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let expected: &[&str] = if moved {
                    &["move", "release", "defer", "marker", "tail", "head"]
                } else {
                    &["defer", "marker", "tail", "release", "head"]
                };
                assert_eq!(events, expected);
            }
            let two = lower("two");
            let mir = two
                .mir()
                .expect("two protected fields have separate obligations");
            let drops = path_drops(mir, mir.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = drops.as_slice() else {
                panic!("expected only the outer invocation");
            };
            let body = mir.disposer(implementation).unwrap();
            let blocks = straight_blocks(body, body.entry);
            assert!(matches!(
                blocks.last().unwrap().terminator,
                Terminator::Return
            ));
            let mut fields = Vec::new();
            for block in blocks {
                if let Operation::Cleanup(actions) = &block.operation {
                    for action in actions {
                        if let Some(operand) = is_release(*action) {
                            let member = operand.field().unwrap();
                            if fields.is_empty() {
                                assert_explicit(block, b"dispose(value.firstLock)");
                            }
                            fields.push(&member.id);
                        }
                    }
                }
            }
            assert_eq!(fields, [&field("firstLock").id, &field("secondLock").id]);
            let limits = AnalysisLimits {
                max_work: branch.statistics().work,
                max_entries: branch.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("branch", limits).completion(), Completion::Checked);
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
                let limited = check("branch", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            for (name, code) in [
                ("duplicateRelease", "R0200"),
                ("outside", "R0501"),
                ("outsideExplicit", "R0501"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.findings()[0].code(), code, "{name}");
            }
            for (name, site) in [
                ("borrowedWrapper", "&value"),
                ("pass", "value"),
                ("callArgument", "value"),
                ("callOutput", "pass(value)"),
                ("replaceField", "active.plainLock"),
                ("otherType", "value: Other"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none(), "{name}");
                assert_eq!(
                    std::str::from_utf8(report.blocked_at().unwrap().span.bytes()),
                    Ok(site),
                    "{name}"
                );
            }
            let ref_only = lower("wrapperReferenceOnly");
            let mir = ref_only
                .mir()
                .expect("the earlier reference-only transfer remains checked");
            assert!(path_drops(mir, mir.entry).is_empty());
            // ri-px1.6 (D1): the mixed signature now checks; its order and caps
            // are pinned in `mixed_wrapper.rs`.
            let mixed = lower("mixedWrapper");
            let mir = mixed.mir().expect("the shared wrapper view is exported");
            let drops = path_drops(mir, mir.entry);
            let [Obligation::Drop(tail), guard, Obligation::Drop(head)] = drops.as_slice() else {
                panic!("expected the owned wrapper's reverse field cleanup: {drops:?}");
            };
            for (place, expected) in [
                (tail, "plainTail"),
                (&is_release(*guard).unwrap(), "plainLock"),
                (head, "plainHead"),
            ] {
                assert_eq!(place.field().unwrap().id, field(expected).id);
                assert_eq!(place.root.span.bytes(), b"owned: Plain");
            }
        },
    );
}

#[test]
fn active_disposer_cleans_only_remaining_fields() {
    probe_with_typed(
        "disposers::active_disposer_cleans_only_remaining_fields",
        r#"
            import std::core::dispose as release;
            type Bag = { head: string; transferred: string; manual: string; tail: string; }
            implement release(value: Bag): void {
                let saved = value.transferred;
                dispose(value.manual);
            }
            function normal(value: Bag, later: string): void { let inner = "inner"; }
            function explicit(value: Bag, later: string): void { release(value); }
            function groupedExplicit(value: Bag): void { dispose<Bag>((value)); }
            function explicitTwice(value: Bag): void { dispose(value); release(value); }
            function explicitLive(value: Bag): &string {
                let view = &value.head;
                release(value);
                return view;
            }

            type Recursive = { text: string; }
            implement release(value: Recursive): void { dispose(value); }
            function recursiveDisposal(value: Recursive): void { release(value); }
            function replace(old: Bag, incoming: Bag): void {
                let mut current = old;
                current = incoming;
            }
            function replaceField(value: Bag): void {
                let mut local = value;
                local.tail = "new";
            }
            function pass(value: Bag): Bag { return value; }
            function cleanResult(value: Bag): void { let _ = pass(value); }
            function liveFieldLoan(value: Bag, seed: &string): &string {
                let mut view = seed;
                if (true) {
                    let local = value;
                    view = &local.head;
                }
                return view;
            }

            type Split = { flag: bool; first: string; rest: string; }
            implement release(value: Split): void {
                let active = value;
                if (active.flag) { let saved = active.first; return; }
            }
            function split(value: Split): void { }

            type Ordered = { text: string; }
            implement release(value: Ordered): void {
                if (true) { let inner = value; }
                let marker = "later";
                dispose(marker);
            }
            function ordered(value: Ordered): void { }

            type Escape = { text: string; }
            function take(value: Escape): void { }
            implement release(value: Escape): void { let active = value; take(active); }
            function triesEscape(value: Escape): void { }

            type Invalid = { text: string; }
            implement release(value: Invalid): void {
                let taken = value.text;
                let again = value.text;
            }
            function triesInvalid(value: Invalid): void { }

            type Protected = { lock: LockGuard<i32>; }
            implement release(value: Protected): void { }
            function protectedCleanup(value: Protected): void { }
        "#,
        |_, module, receipt, typed| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let explicit = lower("explicit");
            let mir = explicit
                .mir()
                .unwrap_or_else(|| panic!("explicit: {:?}", explicit.completion()));
            let cleanup = path_drops(mir, mir.entry);
            let [
                Obligation::InvokeDisposer {
                    operand,
                    implementation,
                },
                Obligation::Drop(later),
            ] = cleanup.as_slice()
            else {
                panic!("explicit disposal must run before ordinary scope cleanup");
            };
            assert_eq!(operand.root.span.bytes(), b"value: Bag");
            assert_eq!(later.root.span.bytes(), b"later: string");
            assert!(mir.disposer(implementation).is_some());
            let invocation = mir
                .blocks
                .iter()
                .find(|block| matches!(&block.operation, Operation::Cleanup(actions)
                    if actions.iter().any(|action| matches!(action, Obligation::InvokeDisposer { .. }))))
                .expect("explicit invocation");
            assert_eq!(invocation.site.span.bytes(), b"release(value)");
            let fact = typed
                .bodies()
                .node(module, invocation.site.node)
                .unwrap()
                .expect("typed call site");
            let compiler::analysis::scalars::NodeFactKind::Expression(value) = &fact.kind else {
                panic!("expected expression fact");
            };
            let call = value.call.as_deref().expect("typed explicit call");
            assert_eq!(
                call.callee,
                &typed.dispose_role().expect("canonical dispose").id
            );
            assert_eq!(
                call.selection
                    .as_ref()
                    .expect("proved implementation")
                    .implementation,
                *implementation
            );
            for (name, code) in [
                ("explicitTwice", "R0200"),
                ("recursiveDisposal", "R0501"),
                ("explicitLive", "R0203"),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected ownership diagnostic")
                };
                assert_eq!(finding.code(), code);
            }

            let grouped = lower("groupedExplicit");
            let grouped_mir = grouped
                .mir()
                .expect("explicit type and grouped named operand");
            assert!(
                matches!(path_drops(grouped_mir, grouped_mir.entry).as_slice(),
                [Obligation::InvokeDisposer { operand, implementation: selected }]
                    if operand.root.span.bytes() == b"value: Bag" && selected == implementation)
            );

            let limits = AnalysisLimits {
                max_work: explicit.statistics().work,
                max_entries: explicit.statistics().entries,
                ..AnalysisLimits::default()
            };
            let declaration = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("explicit"))
                .unwrap();
            let at_limits = |limits| lower_body(receipt, module, &declaration.id, limits).unwrap();
            assert!(at_limits(limits).mir().is_some());
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
                let limited = at_limits(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

            let checked = lower("normal");
            let mir = checked
                .mir()
                .unwrap_or_else(|| panic!("{:?}", checked.completion()));
            let cleanup = path_drops(mir, mir.entry);
            let [
                Obligation::Drop(inner),
                Obligation::Drop(later),
                Obligation::InvokeDisposer {
                    operand,
                    implementation,
                },
            ] = cleanup.as_slice()
            else {
                panic!("expected inner cleanup, then reverse parameter order: {cleanup:?}");
            };
            assert_eq!(inner.root.span.bytes(), b"inner");
            assert_eq!(later.root.span.bytes(), b"later: string");
            assert_eq!(operand.root.span.bytes(), b"value: Bag");
            let implementation = module
                .declarations
                .iter()
                .find(|decl| &decl.id == *implementation)
                .unwrap();
            assert!(
                implementation
                    .span
                    .bytes()
                    .starts_with(b"implement release(value: Bag)")
            );

            let active = mir
                .disposer(&implementation.id)
                .expect("selected disposer MIR");
            assert_eq!(
                active
                    .blocks
                    .iter()
                    .filter(|b| matches!(b.operation, Operation::Dispose { operand, .. }
                if operand.field().is_some_and(|field| field.name.as_deref() == Some("manual"))))
                    .count(),
                1
            );
            let cleanup = path_drops(active, active.entry);
            let [
                Obligation::Drop(saved),
                Obligation::Drop(tail),
                Obligation::Drop(head),
            ] = cleanup.as_slice()
            else {
                panic!(
                    "expected the moved value and only the remaining initialized fields: {cleanup:?}"
                );
            };
            assert_eq!(saved.root.span.bytes(), b"saved");
            assert_eq!(
                tail.field().and_then(|field| field.name.as_deref()),
                Some("tail")
            );
            assert_eq!(
                head.field().and_then(|field| field.name.as_deref()),
                Some("head")
            );

            let replacement = lower("replace");
            let replacement = replacement.mir().expect("replacement MIR");
            let retire = path_drops(replacement, replacement.entry);
            assert_eq!(
                retire.len(),
                2,
                "retire the old and installed values once each"
            );
            assert!(retire.iter().all(|obligation| matches!(obligation,
                Obligation::InvokeDisposer { operand, implementation: selected }
                if *selected == &implementation.id && operand.root.span.bytes() == b"current"
            )));
            assert!(replacement.blocks.iter().any(|b| matches!((&b.operation, b.terminator),
                (Operation::Cleanup(actions), Terminator::Goto(next))
                    if actions.iter().any(|action| matches!(action, Obligation::InvokeDisposer { .. }))
                        && matches!(replacement.blocks[next].operation, Operation::Install { target, .. }
                           if target.root.span.bytes() == b"current")
            )), "disposer invocation must precede installation");

            let split = lower("split");
            let split = split.mir().expect("branching disposer MIR");
            let retire = path_drops(split, split.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = retire.as_slice() else {
                panic!("expected one outer disposer: {retire:?}");
            };
            let active = split.disposer(implementation).unwrap();
            let (yes, no) = branch_targets(active);
            assert!(
                matches!(path_drops(active, yes).as_slice(), [Obligation::Drop(saved), Obligation::Drop(rest)]
                if saved.root.span.bytes() == b"saved" && rest.field().and_then(|f| f.name.as_deref()) == Some("rest"))
            );
            assert!(
                matches!(path_drops(active, no).as_slice(), [Obligation::Drop(rest), Obligation::Drop(first)]
                if rest.field().and_then(|f| f.name.as_deref()) == Some("rest")
                    && first.field().and_then(|f| f.name.as_deref()) == Some("first"))
            );

            let ordered = lower("ordered");
            let ordered = ordered.mir().expect("ordered outer disposer");
            let retire = path_drops(ordered, ordered.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = retire.as_slice() else {
                panic!("expected one outer action: {retire:?}");
            };
            let active = ordered.disposer(implementation).unwrap();
            let deferred = active.blocks.iter().position(|b| matches!(&b.operation, Operation::Cleanup(actions)
                if actions.iter().any(|a| matches!(a, Obligation::DeferDisposerFields { from, .. } if from.root.span.bytes() == b"inner")))).expect("transfer the inner alias's remaining fields to the epilogue");
            let explicit = active
                .blocks
                .iter()
                .position(|b| {
                    matches!(b.operation, Operation::Dispose { operand, .. }
                if operand.root.span.bytes() == b"marker")
                })
                .expect("later explicit disposal");
            let remaining = active.blocks.iter().position(|b| matches!(&b.operation, Operation::Cleanup(actions)
                if actions.iter().any(|a| matches!(a, Obligation::Drop(place) if place.field().is_some_and(|f| f.name.as_deref() == Some("text")))))).expect("remaining field cleanup");
            assert!(
                deferred < explicit && explicit < remaining,
                "retiring the inner alias must not drop fields before the disposer ends"
            );

            let field = lower("replaceField");
            let field = field
                .mir()
                .expect("ordinary custom record's field replacement");
            assert!(
                matches!(path_drops(field, field.entry).as_slice(), [Obligation::Drop(old), Obligation::InvokeDisposer { .. }]
                if old.field().is_some_and(|f| f.name.as_deref() == Some("tail")))
            );
            let result = lower("cleanResult");
            let result = result
                .mir()
                .expect("cleanup after a checked record-valued call");
            assert!(
                result
                    .blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::Call(_)))
            );
            assert!(
                matches!(path_drops(result, result.entry).as_slice(), [Obligation::InvokeDisposer { implementation, .. }]
                if result.disposer(implementation).is_some())
            );

            let protected = lower("protectedCleanup");
            let mir = protected
                .mir()
                .expect("the wrapper's disposer must retain its protected field cleanup");
            let drops = path_drops(mir, mir.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = drops.as_slice() else {
                panic!("the caller must invoke only the outer disposer");
            };
            let body = mir.disposer(implementation).unwrap();
            assert!(
                matches!(path_drops(body, body.entry).as_slice(), [Obligation::ReleaseGuard { operand, .. }]
                if operand.field().is_some_and(|field| field.name.as_deref() == Some("lock")))
            );

            let normal = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("normal"))
                .unwrap();
            let limits = AnalysisLimits {
                max_work: checked.statistics().work - 1,
                ..AnalysisLimits::default()
            };
            let short = lower_body(receipt, module, &normal.id, limits).unwrap();
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(
                short.mir().is_none(),
                "do not publish an unchecked disposer graph"
            );

            for (name, code, witness) in [
                ("triesEscape", "R0501", b"value: Escape".as_slice()),
                ("triesInvalid", "R0200", b"value.text".as_slice()),
                ("liveFieldLoan", "R0203", b"&local.head".as_slice()),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.related.span.bytes(), witness);
            }
        },
    );
}
