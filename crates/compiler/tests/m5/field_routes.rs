use super::*;
use compiler::analysis::ownership::Report;

fn require_mir<'a, 'g>(report: &'a Report<'g>, name: &str) -> &'a Mir<'g> {
    report
        .mir()
        .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()))
}

#[test]
fn observes_borrowed_scalar_temporary_within_its_statement() {
    probe_with_typed(
        "field_routes::observes_borrowed_scalar_temporary_within_its_statement",
        r#"
            function make(bit: bool): bool { return bit; }
            function same(flag: bool): bool {
                let observed = *(&make(flag));
                return observed;
            }
            function grouped(flag: bool): bool {
                let observed = ( ( * ( ( & ( ( make((flag)) ) ) ) ) ) );
                return observed;
            }
            function groupRead(flag: bool): bool { let observed = ((*(&make(flag)))); return observed; }
            function groupBorrow(flag: bool): bool { let observed = *(((&make(flag)))); return observed; }
            function groupProducer(flag: bool): bool { let observed = *(&( (make(flag)) )); return observed; }
            function deepUnsupported(): bool { let observed = *(&((((((true))))))); return observed; }
            function broken(bit: bool): bool {
                let text = "moved";
                dispose(text);
                dispose(text);
                return bit;
            }
            function reached(flag: bool): bool {
                let observed = *(&broken(flag));
                return observed;
            }
            function literal(): bool {
                let observed = *(&true);
                return observed;
            }
        "#,
        |_, module, receipt, _| {
            use compiler::analysis::ownership::Cleanup::EndStorage;

            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let mut plain_depth = 0;
            for name in ["same", "grouped"] {
                let report = check(name, AnalysisLimits::default());
                let mir = require_mir(&report, name);
                assert!(report.findings().is_empty());
                let ops: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| !matches!(op, Operation::None))
                    .collect();
                let [
                    Operation::Copy {
                        from: flag,
                        into: Some(argument),
                    },
                    Operation::CallIntoTemporary {
                        callee,
                        arguments,
                        into: backing,
                    },
                    Operation::Borrow {
                        source,
                        holder,
                        exclusive: false,
                    },
                    Operation::CopyReferent {
                        from: dereferenced,
                        into: observed,
                    },
                    Operation::Cleanup(statement_end),
                    Operation::Copy {
                        from: returned,
                        into: Some(result),
                    },
                    Operation::Export(output),
                    Operation::Cleanup(lexical_end),
                    Operation::Cleanup(parameter_end),
                ] = ops.as_slice()
                else {
                    panic!(
                        "expected argument, call, borrow, observation, statement cleanup and return: {ops:?}"
                    );
                };
                let [call_argument] = arguments.as_slice() else {
                    panic!("expected one call argument");
                };
                assert_eq!(call_argument.root.node, argument.root.node);
                assert_eq!(*callee, &declaration("make").id);
                assert_eq!(flag.root.span.bytes(), b"flag: bool");
                let producer_site: &[u8] = if name == "same" {
                    b"make(flag)"
                } else {
                    b"make((flag))"
                };
                assert_eq!(backing.root.span.bytes(), producer_site);
                assert_ne!(backing.root.node, flag.root.node);
                assert_ne!(backing.root.node, argument.root.node);
                assert_eq!(source.root.node, backing.root.node);
                assert_eq!(holder.root.node, dereferenced.root.node);
                assert!(
                    [backing, source, holder, dereferenced, observed]
                        .iter()
                        .all(|place| place.projection.is_none())
                );
                assert_eq!(observed.root.span.bytes(), b"observed");
                assert_eq!(returned.root.node, observed.root.node);
                assert_eq!(result.root.node, output.root.node);
                assert!(
                    matches!(statement_end.as_slice(), [EndStorage(view), EndStorage(temp)]
                    if view.root.node == holder.root.node && temp.root.node == backing.root.node)
                );
                assert!(
                    matches!(lexical_end.as_slice(), [EndStorage(place)] if place.root.node == observed.root.node)
                );
                assert!(
                    matches!(parameter_end.as_slice(), [EndStorage(place)] if place.root.node == flag.root.node)
                );

                let stats = report.statistics();
                if name == "same" {
                    plain_depth = stats.max_depth;
                } else {
                    assert!(stats.max_depth > plain_depth);
                    let exact = AnalysisLimits {
                        max_work: stats.work,
                        max_entries: stats.entries,
                        max_depth: stats.max_depth,
                    };
                    assert!(check(name, exact).mir().is_some());
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
                        assert_eq!(limited.completion(), Completion::ResourceLimit);
                        assert!(limited.mir().is_none());
                    }
                }
            }
            for name in [
                "groupRead",
                "groupBorrow",
                "groupProducer",
                "deepUnsupported",
            ] {
                let report = check(name, AnalysisLimits::default());
                if name == "deepUnsupported" {
                    assert_eq!(report.completion(), Completion::Incomplete);
                } else {
                    require_mir(&report, name);
                }
                let limited = check(
                    name,
                    AnalysisLimits {
                        max_depth: plain_depth,
                        ..AnalysisLimits::default()
                    },
                );
                assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                assert!(limited.mir().is_none());
            }
            let reached = check("reached", AnalysisLimits::default());
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            assert_eq!(reached.findings()[0].code(), "R0200");

            let literal = check("literal", AnalysisLimits::default());
            assert_eq!(literal.completion(), Completion::Incomplete);
            assert!(literal.mir().is_none());
        },
    );
}

#[test]
fn later_use_requires_a_named_scalar_owner() {
    probe_with_typed(
        "field_routes::later_use_requires_a_named_scalar_owner",
        r#"
            function make(bit: bool): bool { return bit; }
            function named(flag: bool): bool {
                let owner = make(flag);
                let view = &owner;
                return *view;
            }
            function later(flag: bool): bool {
                let view = &make(flag);
                return *view;
            }
            function unused(flag: bool): bool {
                let view = &make(flag);
                return flag;
            }
            function maybeRead(flag: bool): bool {
                let view = &make(flag);
                if (flag) { return *view; }
                return flag;
            }
            function prunedRead(flag: bool): bool {
                let view = &make(flag);
                if (false) { return *view; }
                return flag;
            }
            function returnDescriptor(flag: bool): &bool {
                let view = &make(flag);
                return view;
            }
        "#,
        |_, module, receipt, _| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let named = lower("named");
            assert!(named.findings().is_empty());
            let mir = require_mir(&named, "named");
            assert!(linear(mir, mir.entry).iter().any(
                |op| matches!(op, Operation::Borrow { source, holder, exclusive: false }
                if source.root.span.bytes() == b"owner" && holder.root.span.bytes() == b"view")
            ));

            for name in ["later", "maybeRead", "returnDescriptor"] {
                let later = lower(name);
                assert_eq!(later.completion(), Completion::SourceRejected, "{name}");
                assert!(later.mir().is_none() && later.blocked_at().is_none());
                let [finding] = later.findings() else {
                    panic!("expected one temporary backing release finding for {name}")
                };
                assert_eq!(finding.code(), "R0203");
                assert_eq!(finding.site.span.bytes(), b"let view = &make(flag);");
                assert_eq!(finding.related.span.bytes(), b"&make(flag)");
            }
            let pruned = lower("prunedRead");
            let mir = require_mir(&pruned, "prunedRead");
            assert!(
                !mir.blocks
                    .iter()
                    .any(|block| matches!(block.operation, Operation::CopyReferent { .. }))
            );

            let unused = lower("unused");
            assert!(unused.findings().is_empty());
            let mir = require_mir(&unused, "unused");
            let ops: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| !matches!(op, Operation::None))
                .collect();
            assert!(
                matches!(ops.as_slice(), [Operation::Copy { .. }, Operation::CallIntoTemporary { into, .. },
                Operation::Borrow { source, holder, exclusive: false }, Operation::Cleanup(statement_end),
                Operation::Copy { .. }, Operation::Export(_), Operation::Cleanup(lexical_end), Operation::Cleanup(_)]
                if source.root.node == into.root.node && holder.root.span.bytes() == b"view"
                && matches!(statement_end.as_slice(), [compiler::analysis::ownership::Cleanup::EndStorage(place)] if place.root.node == source.root.node)
                && matches!(lexical_end.as_slice(), [compiler::analysis::ownership::Cleanup::EndStorage(place)] if place.root.node == holder.root.node))
            );
        },
    );
}

#[test]
fn incoming_shared_record_copies_carried_field() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyReference, SharedSlot},
        Projection,
    };

    let mut blockers = Vec::new();
    probe_with_typed(
        "field_routes::incoming_shared_record_copies_carried_field",
        r#"
            type Other = { value: &bool; }
            type Holder = { value: &bool; }
            function project(x: &Holder): &bool { return x.value; }
            function grouped(x: &Holder): &bool { return ((x.value)); }
            function groupedReceiver(x: &Holder): &bool { return (x).value; }
            function alias(x: &Holder): &bool { let saved = x; return saved.value; }
            type Wider = { value: &bool; pair: [bool, bool]; }
            function unsupportedSibling(x: &Wider): &bool { return x.value; }
            function slot(x: &Holder): &&bool { return &x.value; }
            function unsupportedSlot(x: &Wider): &&bool { return &x.value; }
            function forwardSlot(x: &Holder): &&bool {
                let view = &x.value;
                let saved = view;
                return saved;
            }
            function escapeSlotCopy(x: &Holder): &&&bool {
                let view = &x.value;
                return &view;
            }
            function overwriteSlotCopy(x: &Holder, other: &&bool): &&bool {
                let mut view = &x.value;
                let loan = &view;
                view = other;
                let observed = loan;
                return view;
            }
            function overwriteAfterUse(x: &Holder, other: &&bool): &&bool {
                let mut view = &x.value;
                let loan = &view;
                let observed = loan;
                view = other;
                return view;
            }
            function localSlot(x: &Holder): &&bool {
                let view = x.value;
                return &view;
            }
            function liveSlot(x: &Holder, seed: &bool): &bool {
                let mut view = x.value;
                let slot = &view;
                view = seed;
                let stillLive = slot;
                return view;
            }
            function usedSlot(x: &Holder, seed: &bool): &bool {
                let mut view = x.value;
                let slot = &view;
                let last = slot;
                view = seed;
                return view;
            }
            function aliasSlot(x: &Holder): &&bool { let saved = x; return &saved.value; }
            function callerEscape(): &bool {
                let local = true;
                let x = Holder { value: &local };
                return project(&x);
            }
            function callerSlotEscape(seed: &bool): &&bool {
                let x = Holder { value: seed };
                return slot(&x);
            }
        "#,
        |request, module, receipt, typed| {
            let check = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let lower = |name| check(name, AnalysisLimits::default());
            let report = lower("project");
            if report.completion() != Completion::Checked {
                blockers.push((
                    request.test_mode,
                    report.completion(),
                    report
                        .blocked_at()
                        .map(|site| site.span.bytes().escape_ascii().to_string()),
                ));
            } else {
                let mir = require_mir(&report, "project");
                assert!(report.findings().is_empty());
                let record = typed
                    .inventory()
                    .type_facts()
                    .declarations()
                    .find(|facts| {
                        facts.declaration.id.module == module.key
                            && facts.declaration.name.as_deref() == Some("Holder")
                    })
                    .unwrap();
                let compiler::analysis::DeclarationShape::Record { fields } = &record.shape else {
                    panic!("expected canonical Holder fields");
                };
                let ops = linear(mir, mir.entry);
                let [
                    Operation::BindBorrowedMember {
                        descriptor,
                        projection: Projection::Field(field),
                        into,
                        binding: CopyReference,
                    },
                    Operation::Export(output),
                ] = ops
                    .iter()
                    .copied()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::BindBorrowedMember { .. } | Operation::Export(_)
                        )
                    })
                    .collect::<Vec<_>>()
                    .as_slice()
                else {
                    panic!("expected a single copy through the incoming descriptor: {ops:?}");
                };
                assert_eq!(descriptor.root.span.bytes(), b"x: &Holder");
                assert!(descriptor.projection.is_none());
                assert_eq!(field.name.as_deref(), Some("value"));
                assert_eq!(field.id, fields[0].declaration.id);
                assert_eq!(output.root.node, into.root.node);
                assert!(output.projection.is_none() && into.projection.is_none());
                assert!(
                    !ops.iter().any(|op| matches!(
                        op,
                        Operation::Borrow { .. } | Operation::Reborrow { .. }
                    ))
                );
                assert!(path_drops(mir, mir.entry).is_empty());

                let slot = lower("slot");
                let mir = require_mir(&slot, "slot");
                assert!(slot.findings().is_empty());
                let ops = linear(mir, mir.entry);
                let [
                    Operation::BindBorrowedMember {
                        descriptor,
                        projection: Projection::Field(slot_field),
                        into,
                        binding: SharedSlot,
                    },
                    Operation::Export(output),
                ] = ops
                    .iter()
                    .copied()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::BindBorrowedMember { .. } | Operation::Export(_)
                        )
                    })
                    .collect::<Vec<_>>()
                    .as_slice()
                else {
                    panic!("expected a single slot view through the incoming descriptor: {ops:?}");
                };
                assert_eq!(descriptor.root.span.bytes(), b"x: &Holder");
                assert!(descriptor.projection.is_none());
                assert_eq!(slot_field.id, field.id);
                assert_eq!(output.root.node, into.root.node);
                assert!(output.projection.is_none() && into.projection.is_none());
                assert!(!ops.iter().any(|op| matches!(
                    op,
                    Operation::Borrow { .. } | Operation::Reborrow { .. } | Operation::Copy { .. }
                )));
                assert!(path_drops(mir, mir.entry).is_empty());
            }
            if report.completion() == Completion::Checked {
                let forwarded = lower("forwardSlot");
                let mir = require_mir(&forwarded, "forwardSlot");
                let route: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::BindBorrowedMember { .. }
                                | Operation::Borrow { .. }
                                | Operation::Reborrow { .. }
                                | Operation::Move { .. }
                                | Operation::Copy { .. }
                                | Operation::Export(_)
                        )
                    })
                    .collect();
                assert!(
                    matches!(route.as_slice(), [Operation::BindBorrowedMember { binding: SharedSlot, into: view, .. },
                    Operation::Copy { from: read, into: Some(saved) }, Operation::Copy { from: returned, into: Some(staged) }, Operation::Export(output)]
                    if view.root.span.bytes() == b"view" && view.root.node == read.root.node && saved.root.span.bytes() == b"saved"
                        && returned.root.node == saved.root.node && staged.root.node == output.root.node)
                );
                for (name, code, site, related) in [
                    (
                        "escapeSlotCopy",
                        "R0202",
                        b"return &view;".as_slice(),
                        b"view".as_slice(),
                    ),
                    ("overwriteSlotCopy", "R0203", b"view = other;", b"&view"),
                ] {
                    let rejected = lower(name);
                    assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                    assert!(rejected.mir().is_none());
                    let [finding] = rejected.findings() else {
                        panic!("expected one finding for {name}")
                    };
                    assert_eq!(finding.code(), code);
                    assert_eq!(finding.site.span.bytes(), site);
                    assert_eq!(finding.related.span.bytes(), related);
                }
                let released = lower("overwriteAfterUse");
                assert!(
                    require_mir(&released, "overwriteAfterUse")
                        .blocks
                        .iter()
                        .any(|block| matches!(&block.operation,
                    Operation::Install { target, .. } if target.root.span.bytes() == b"view"))
                );
                for name in ["project", "slot"] {
                    let stats = lower(name).statistics();
                    let exact = AnalysisLimits {
                        max_work: stats.work,
                        max_entries: stats.entries,
                        max_depth: stats.max_depth,
                    };
                    assert!(check(name, exact).mir().is_some());
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
                        assert!(limited.mir().is_none());
                    }
                }
                assert!(lower("grouped").mir().is_some());
                let limited = check(
                    "grouped",
                    AnalysisLimits {
                        max_depth: report.statistics().max_depth,
                        ..AnalysisLimits::default()
                    },
                );
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
                let local_slot = lower("localSlot");
                assert_eq!(local_slot.completion(), Completion::SourceRejected);
                assert!(local_slot.mir().is_none());
                let [finding] = local_slot.findings() else {
                    panic!("expected an escaping local-slot borrow");
                };
                assert_eq!(finding.code(), "R0202");
                assert_eq!(finding.site.span.bytes(), b"return &view;");
                assert_eq!(finding.related.span.bytes(), b"view");

                let live_slot = lower("liveSlot");
                assert_eq!(live_slot.completion(), Completion::SourceRejected);
                assert!(live_slot.mir().is_none());
                let [finding] = live_slot.findings() else {
                    panic!("expected a conflict with the live slot borrow");
                };
                assert_eq!(finding.code(), "R0203");
                assert_eq!(finding.site.span.bytes(), b"view = seed;");
                assert_eq!(finding.related.span.bytes(), b"&view");

                let used_slot = lower("usedSlot");
                let mir = require_mir(&used_slot, "usedSlot");
                assert!(used_slot.findings().is_empty());
                let ops = linear(mir, mir.entry);
                let borrow = ops
                    .iter()
                    .position(|op| {
                        matches!(op, Operation::Borrow {
                    source, exclusive: false, ..
                } if source.root.span.bytes() == b"view" && source.projection.is_none())
                    })
                    .unwrap();
                let last_read = ops
                    .iter()
                    .position(|op| {
                        matches!(op, Operation::Copy {
                    from, ..
                } if from.root.span.bytes() == b"slot" && from.projection.is_none())
                    })
                    .unwrap();
                let install = ops
                    .iter()
                    .position(|op| {
                        matches!(op, Operation::Install {
                    target, ..
                } if target.root.span.bytes() == b"view" && target.projection.is_none())
                    })
                    .unwrap();
                assert!(borrow < last_read && last_read < install);
            }
            for (name, site) in [
                ("callerEscape", b"return project(&x);".as_slice()),
                ("callerSlotEscape", b"return slot(&x);"),
            ] {
                let report = lower(name);
                assert_rejected(&report, name, "R0202", site, b"x");
            }
            for (name, blocked) in [
                ("aliasSlot", b"saved.value".as_slice()),
                ("unsupportedSlot", b"x.value"),
                ("alias", b"saved.value"),
                ("groupedReceiver", b"(x).value"),
                ("unsupportedSibling", b"x.value"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert_eq!(report.blocked_at().unwrap().span.bytes(), blocked, "{name}");
                assert!(report.mir().is_none(), "{name}");
            }
        },
    );
    assert!(blockers.is_empty(), "{blockers:?}");
}

fn assert_rejected(report: &Report<'_>, name: &str, code: &str, site: &[u8], related: &[u8]) {
    assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
    assert!(report.mir().is_none(), "{name}");
    let [finding] = report.findings() else {
        panic!("{name}: expected one finding");
    };
    assert_eq!(finding.code(), code, "{name}");
    assert_eq!(finding.site.span.bytes(), site, "{name}: primary");
    assert_eq!(finding.related.span.bytes(), related, "{name}: related");
}

#[test]
fn borrowed_result_calls_forward_and_use_shared_record_inputs() {
    use compiler::analysis::{DeclarationShape, TypeTerm, scalars::NodeFactKind};
    probe_with_typed(
        "field_routes::borrowed_result_calls_forward_and_use_shared_record_inputs",
        r#"
            type Holder = { value: &bool; }
            function project(x: &Holder): &bool { return x.value; }
            function slot(x: &Holder): &&bool { return &x.value; }
            function projectIncoming(x: &Holder): &bool { return project(x); }
            function slotIncoming(x: &Holder): &&bool { return slot(x); }
            function projectUse(): bool {
                let flag = true;
                let x = Holder { value: &flag };
                let result = project(&x);
                return *result;
            }
            function slotUse(): bool {
                let flag = true;
                let x = Holder { value: &flag };
                let result = slot(&x);
                let last = result;
                return true;
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
            let signature = |id| {
                typed
                    .inventory()
                    .type_facts()
                    .declarations()
                    .find(|facts| &facts.declaration.id == id)
                    .unwrap()
            };
            let DeclarationShape::Record { fields } = &signature(&declaration("Holder").id).shape
            else {
                panic!("expected the canonical Holder record");
            };
            let [field] = fields.as_slice() else {
                panic!("expected one field")
            };
            assert!(
                matches!(field.ty.as_ref(), TypeTerm::Reference { mutable: false, referent }
                if matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
            );
            for (name, target, incoming) in [
                ("projectIncoming", "project", true),
                ("slotIncoming", "slot", true),
                ("projectUse", "project", false),
                ("slotUse", "slot", false),
            ] {
                let lower =
                    |limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
                let report = lower(AnalysisLimits::default());
                let mir = require_mir(&report, name);
                assert!(report.findings().is_empty(), "{name}");
                let blocks = straight_blocks(mir, mir.entry);
                let calls: Vec<_> = blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, block)| match &block.operation {
                        Operation::CallBorrowingRecord {
                            callee,
                            argument,
                            into,
                        } => Some((i, *callee, argument, into)),
                        _ => None,
                    })
                    .collect();
                let &[(index, callee, argument, into)] = calls.as_slice() else {
                    panic!("{name}: expected one borrowed-result call");
                };
                assert_eq!(callee, &declaration(target).id);
                let DeclarationShape::Function { parameters, result } = &signature(callee).shape
                else {
                    panic!("expected the selected application function");
                };
                let [formal] = parameters.as_slice() else {
                    panic!("expected one shared input")
                };
                assert!(
                    matches!(formal.ty.as_ref(), TypeTerm::Reference { mutable: false, referent }
                    if matches!(referent.as_ref(), TypeTerm::Nominal { declaration: record, arguments }
                        if *record == &declaration("Holder").id && arguments.is_empty()))
                );
                if target == "project" {
                    assert_eq!(*result, field.ty);
                } else {
                    assert!(
                        matches!(result.as_ref(), TypeTerm::Reference { mutable: false, referent } if *referent == field.ty)
                    );
                }
                let site = blocks[index].site;
                let NodeFactKind::Expression(selected) = &typed
                    .bodies()
                    .node(module, site.node)
                    .unwrap()
                    .unwrap()
                    .kind
                else {
                    panic!("expected typed call evidence");
                };
                let call = selected.call.as_deref().unwrap();
                assert_eq!(call.callee, callee);
                assert_eq!(selected.ty, *result);
                assert_eq!(call.arguments.as_slice(), [argument.root.node]);
                assert!(call.substitution.is_empty() && call.pipe_input.is_none());
                let NodeFactKind::Expression(actual) = &typed
                    .bodies()
                    .node(module, argument.root.node)
                    .unwrap()
                    .unwrap()
                    .kind
                else {
                    panic!("expected typed argument evidence");
                };
                assert_eq!(actual.ty, formal.ty);
                assert!(argument.projection.is_none() && into.projection.is_none());

                let stages: Vec<_> = blocks[..index]
                    .iter()
                    .filter_map(|block| match &block.operation {
                        Operation::Copy {
                            into: Some(place), ..
                        }
                        | Operation::Borrow { holder: place, .. }
                            if place.root.node == argument.root.node =>
                        {
                            Some(&block.operation)
                        }
                        _ => None,
                    })
                    .collect();
                let &[stage] = stages.as_slice() else {
                    panic!("{name}: expected one argument evaluation")
                };
                let ended: Vec<_> = blocks[index + 1..]
                    .iter()
                    .filter_map(|block| match &block.operation {
                        Operation::Cleanup(actions) => Some(actions),
                        _ => None,
                    })
                    .flatten()
                    .filter_map(|op| match op {
                        Obligation::EndStorage(place) => Some(place),
                        _ => None,
                    })
                    .collect();
                assert!(
                    !ended
                        .iter()
                        .any(|place| place.root.node == argument.root.node)
                );
                if incoming {
                    let Operation::Copy { from, .. } = stage else {
                        panic!("expected a descriptor copy")
                    };
                    assert_eq!(from.root.span.bytes(), b"x: &Holder");
                    assert!(
                        matches!(&blocks[index + 1].operation, Operation::Export(output) if output.root.node == into.root.node)
                    );
                    assert_eq!(ended.len(), 1);
                    assert_eq!(ended[0].root.node, from.root.node);
                } else {
                    let Operation::Borrow {
                        source,
                        exclusive: false,
                        ..
                    } = stage
                    else {
                        panic!("expected a whole shared borrow")
                    };
                    assert_eq!(source.root.span.bytes(), b"x");
                    assert!(source.projection.is_none());
                    assert_eq!(into.root.span.bytes(), b"result");
                    assert!(match target {
                        "project" =>
                            matches!(&blocks[index + 1].operation, Operation::Dereference(from) if from.root.node == into.root.node),
                        _ =>
                            matches!(&blocks[index + 1].operation, Operation::Copy { from, into: Some(last) } if from.root.node == into.root.node && last.root.span.bytes() == b"last"),
                    });
                    assert!(matches!(ended.as_slice(), [.., result, member, whole, flag]
                        if result.root.node == into.root.node && member.root.node == source.root.node
                            && matches!(member.projection, Some(compiler::analysis::ownership::Projection::Field(f)) if f.id == field.declaration.id)
                            && whole.root.node == source.root.node && whole.projection.is_none()
                            && flag.root.span.bytes() == b"flag"));
                }
                assert!(path_drops(mir, mir.entry).is_empty());

                let stats = report.statistics();
                let exact = AnalysisLimits {
                    max_work: stats.work,
                    max_entries: stats.entries,
                    max_depth: stats.max_depth,
                };
                assert!(lower(exact).mir().is_some(), "{name}: exact bounds");
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
                    let report = lower(short);
                    assert_eq!(report.completion(), Completion::ResourceLimit, "{name}");
                    assert!(report.mir().is_none(), "{name}");
                }
            }
        },
    );
}

#[test]
fn borrowed_result_calls_retain_storage_and_all_contained_sources_until_last_use() {
    probe(
        "field_routes::borrowed_result_calls_retain_storage_and_all_contained_sources_until_last_use",
        r#"
            type Holder = { value: &bool; }
            function project(x: &Holder): &bool { return x.value; }
            function slot(x: &Holder): &&bool { return &x.value; }
            function extract(x: Holder): &bool { return x.value; }
            function extractIncoming(seed: &bool): &bool {
                let x = Holder { value: seed };
                return extract(x);
            }
            function extractLocal(): &bool {
                let flag = true;
                let x = Holder { value: &flag };
                return extract(x);
            }
            function projectEscape(seed: &bool): &bool {
                let x = Holder { value: seed };
                return project(&x);
            }
            function slotReplacement(seed: &bool): bool {
                let mut x = Holder { value: seed };
                let result = slot(&x);
                let copy = result;
                x.value = seed;
                let last = copy;
                return true;
            }
            function slotReleased(seed: &bool): bool {
                let mut x = Holder { value: seed };
                let result = slot(&x);
                let copy = result;
                let last = copy;
                x.value = seed;
                return true;
            }
            function recordExpiry(seed: &bool): bool {
                let mut result = seed;
                { let x = Holder { value: seed }; result = project(&x); }
                return *result;
            }
            function carriedExpiry(seed: &bool): bool {
                let mut x = Holder { value: seed };
                let mut result = seed;
                { let flag = true; x.value = &flag; result = project(&x); }
                return *result;
            }
            function carriedReleased(seed: &bool): bool {
                let mut x = Holder { value: seed };
                let mut observed = false;
                {
                    let flag = true;
                    x.value = &flag;
                    let result = project(&x);
                    observed = *result;
                }
                return observed;
            }
            type Two = { value: &bool; unused: &bool; }
            function first(x: &Two): &bool { return x.value; }
            function firstSlot(x: &Two): &&bool { return &x.value; }
            function restrictedCopy(seed: &bool): bool {
                let mut flag = true;
                let access = &mut flag;
                let x = Two { value: seed, unused: &*access };
                let result = first(&x);
                let read = flag;
                return *result;
            }
            function restrictedSlot(seed: &bool): bool {
                let mut flag = true;
                let access = &mut flag;
                let x = Two { value: seed, unused: &*access };
                let result = firstSlot(&x);
                let read = flag;
                let last = result;
                return read;
            }
            function restrictionReleased(seed: &bool): bool {
                let mut flag = true;
                let access = &mut flag;
                let x = Two { value: seed, unused: &*access };
                let result = first(&x);
                let last = *result;
                return flag;
            }
            function invalid(x: &Holder): &&bool {
                let view = x.value;
                return &view;
            }
            function reachesInvalid(x: &Holder): bool {
                let unused = invalid(x);
                return true;
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
            for (name, code, site, related) in [
                ("extractLocal", "R0202", "return extract(x);", "flag"),
                ("projectEscape", "R0202", "return project(&x);", "x"),
                ("slotReplacement", "R0203", "x.value = seed;", "&x"),
                (
                    "recordExpiry",
                    "R0203",
                    "{ let x = Holder { value: seed }; result = project(&x); }",
                    "&x",
                ),
                (
                    "carriedExpiry",
                    "R0203",
                    "{ let flag = true; x.value = &flag; result = project(&x); }",
                    "&flag",
                ),
                ("restrictedCopy", "R0201", "flag", "&mut flag"),
                ("restrictedSlot", "R0201", "flag", "&mut flag"),
                ("reachesInvalid", "R0202", "return &view;", "view"),
            ] {
                assert_rejected(
                    &lower(name),
                    name,
                    code,
                    site.as_bytes(),
                    related.as_bytes(),
                );
            }
            for name in ["slotReleased", "carriedReleased", "restrictionReleased"] {
                let report = lower(name);
                require_mir(&report, name);
                assert!(report.findings().is_empty(), "{name}");
            }
            let owned = lower("extractIncoming");
            let mir = require_mir(&owned, "extractIncoming");
            assert!(owned.findings().is_empty());
            let ops = linear(mir, mir.entry);
            assert!(ops.iter().any(|op| matches!(op, Operation::Call(callee)
                if module.declarations.iter().any(|d| d.name.as_deref() == Some("extract") && d.id == **callee))));
            assert!(!ops.iter().any(|op| matches!(
                op,
                Operation::Borrow { .. } | Operation::CallBorrowingRecord { .. }
            )));
        },
    );
}

#[test]
fn borrowed_result_calls_require_the_exact_direct_shared_signature() {
    probe(
        "field_routes::borrowed_result_calls_require_the_exact_direct_shared_signature",
        r#"
            type Holder = { value: &bool; }
            type Deep = { value: &bool; sibling: &&bool; }
            type Custom = { value: &bool; }
            implement dispose(x: Custom): void {}
            function project(x: &Holder): &bool { return x.value; }
            function scalar(x: &Holder): bool { return true; }
            function extra(x: &Holder, flag: bool): &bool { return x.value; }
            function generic<T>(x: &Holder): &bool { return x.value; }
            function writable(x: &mut Holder): &bool { return x.value; }
            function deep(x: &Deep): &bool { return x.value; }
            function custom(x: &Custom): &bool { return x.value; }
            function wrong(x: &Holder): &i32 { let local: i32 = 1; return &local; }
            function noBorrowResult(x: &Holder): bool { return scalar(x); }
            function extraInput(x: &Holder): &bool { return extra(x, true); }
            function genericCall(x: &Holder): &bool { return generic<bool>(x); }
            function pipe(x: &Holder): &bool { return x |> project(_); }
            function writableInput(x: &mut Holder): &bool { return writable(x); }
            function deepInput(x: &Deep): &bool { return deep(x); }
            function customInput(x: &Custom): &bool { return custom(x); }
            function wrongResult(x: &Holder): &i32 { return wrong(x); }
            function alias(x: &Holder): &bool { let saved = x; return project(saved); }
        "#,
        |_, module, receipt| {
            for (name, site) in [
                ("noBorrowResult", "x"),
                ("extraInput", "x"),
                ("genericCall", "generic<bool>(x)"),
                ("pipe", "x |> project(_)"),
                ("writableInput", "x"),
                ("deepInput", "deep(x)"),
                ("customInput", "custom(x)"),
                ("wrongResult", "wrong(x)"),
                ("alias", "saved"),
            ] {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(
                    report.mir().is_none() && report.findings().is_empty(),
                    "{name}"
                );
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    site.as_bytes(),
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn carried_field_and_slot_borrow_differ() {
    probe(
        "field_routes::carried_field_and_slot_borrow_differ",
        r#"
            type Holder = { value: &bool; }
            type WithText = { text: string; value: &bool; }
            function forward(x: Holder): &bool { return x.value; }
            function escapes(x: Holder): &&bool { return &x.value; }
            function forwardAndClean(x: WithText): &bool { return x.value; }
            function escapeLocal(): &bool {
                let local = true;
                let x = Holder { value: &local };
                return x.value;
            }
            function escapeInRecord(): Holder {
                let local = true;
                return Holder { value: &local };
            }
            function callerForward(seed: &bool, text: string): &bool {
                let x = WithText { text: text, value: seed };
                return forwardAndClean(x);
            }
            function callerEscapes(): &bool {
                let local = true;
                let x = Holder { value: &local };
                return forward(x);
            }
            function callerRetains(): bool {
                let mut local = true;
                let x = Holder { value: &local };
                let view = forward(x);
                local = false;
                return *view;
            }
            function fieldReplacement(seed: &bool): &bool {
                let mut local = true;
                let mut x = Holder { value: &local };
                x.value = seed;
                local = false;
                return x.value;
            }
            function otherHolder(seed: &bool): bool {
                let mut local = true;
                let mut x = Holder { value: &local };
                let other = x;
                x.value = seed;
                local = false;
                return *other.value;
            }
            function storageBeforeSource(seed: &bool): void {
                let mut x = Holder { value: seed };
                let local = true;
                x.value = &local;
            }
            type Two = { first: &bool; second: &bool; }
            function possibleSource(seed: &bool): bool {
                let mut local = true;
                let mut x = Two { first: &local, second: seed };
                let carried = x.second;
                x.first = seed;
                local = false;
                return *carried;
            }
            function futureProjection(seed: &bool): &bool {
                let mut local = true;
                let x = Two { first: &local, second: seed };
                local = false;
                return x.second;
            }
            function reborrowEscape(seed: &bool): &bool {
                let local = true;
                let x = Two { first: &local, second: seed };
                return &*x.second;
            }
            function reborrowAfterRelease(seed: &bool): &bool {
                let mut local = true;
                let x = Two { first: &local, second: seed };
                local = false;
                return &*x.second;
            }
            function groupedReborrow(x: Holder): &bool { return &(*x.value); }
            function borrowHolder(): void {
                let mut local = true;
                let x = Holder { value: &local };
                let access = &x;
                local = false;
                let last = access;
            }
            function freeze(source: &mut bool): &bool { return &*source; }
            function retainsExclusiveRestriction(): bool {
                let mut local = true;
                let view = freeze(&mut local);
                let x = Holder { value: view };
                let holder = x;
                let before = local;
                return *holder.value;
            }
            function branchRetains(flag: bool, seed: &bool): bool {
                let mut local = true;
                let mut x = Holder { value: seed };
                if (flag) { x.value = &local; }
                local = false;
                return *x.value;
            }
            function loopEscapes(flag: bool, seed: &bool): &bool {
                let mut x = Holder { value: seed };
                while (flag) {
                    let local = true;
                    x.value = &local;
                    break;
                }
                return x.value;
            }
            function loopClears(flag: bool, seed: &bool): &bool {
                let mut x = Holder { value: seed };
                while (flag) {
                    let local = true;
                    x.value = &local;
                    let used = *x.value;
                    x.value = seed;
                    continue;
                }
                return x.value;
            }
            type LocalView = { owned: bool; view: &bool; }
            function groupedOnly(seed: &bool): void {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
            }
            type TwoViews = { owned: bool; internal: &bool; external: &bool; }
            function mixedEdges(seed: &bool): void {
                let mut holder = TwoViews { owned: true, internal: seed, external: seed };
                let backing = LocalView { owned: false, view: seed };
                holder.internal = &holder.owned;
                holder.external = &backing.owned;
            }
            type ReverseView = { view: &bool; owned: bool; }
            function outOfOrder(seed: &bool): void {
                let mut x = ReverseView { owned: true, view: seed };
                x.view = &x.owned;
            }
            function recordCycle(seed: &bool): void {
                let mut x = LocalView { owned: true, view: seed };
                let mut y = LocalView { owned: false, view: seed };
                x.view = &y.owned;
                y.view = &x.owned;
            }
            type TextView = { owned: bool; view: &bool; text: string; }
            function observable(seed: &bool, text: string): void {
                let mut x = TextView { owned: true, view: seed, text: text };
                x.view = &x.owned;
            }
            function moveOwner(seed: &bool, text: string): bool {
                let mut x = TextView { owned: true, view: seed, text: text };
                x.view = &x.owned;
                let moved = x;
                return *moved.view;
            }
            type GuardView = { owned: bool; view: &bool; lock: LockGuard<i32>; }
            function protected(seed: &bool, lock: LockGuard<i32>): void {
                let mut x = GuardView { owned: true, view: seed, lock: lock };
                x.view = &x.owned;
            }
            function replaceOwner(seed: &bool): void {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
                x = LocalView { owned: false, view: seed };
            }
            function usedCopy(seed: &bool): bool {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
                let copied = x.view;
                x.view = seed;
                let observed = *copied;
                x.owned = false;
                return observed;
            }
            function liveCopy(seed: &bool): bool {
                let mut x = LocalView { owned: true, view: seed };
                x.view = &x.owned;
                let copied = x.view;
                x.view = seed;
                x.owned = false;
                return *copied;
            }
            function liveAcrossBoundary(seed: &bool): bool {
                let mut copied = seed;
                {
                    let mut x = LocalView { owned: true, view: seed };
                    x.view = &x.owned;
                    copied = x.view;
                }
                return *copied;
            }
        "#,
        |_, module, receipt| {
            let lower_with = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let lower = |name| lower_with(name, AnalysisLimits::default());

            let forwarded = lower("forward");

            let mir = require_mir(&forwarded, "forward");
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Copy { from, into: Some(_) }
                if from.root.span.bytes() == b"x: Holder" && from.field().is_some_and(|field| field.name.as_deref() == Some("value")))));
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Export(place) if place.root.span.bytes() == b"x.value")));
            assert!(mir.blocks.iter().any(|block| matches!(&block.operation, Operation::Cleanup(cleanup)
                if matches!(cleanup.as_slice(), [Obligation::EndStorage(field), Obligation::EndStorage(root)]
                    if field.field().is_some_and(|decl| decl.name.as_deref() == Some("value"))
                        && root.projection.is_none() && root.root.span.bytes() == b"x: Holder"))));
            assert!(path_drops(mir, mir.entry).is_empty());

            let mixed = lower("forwardAndClean");

            let mir = require_mir(&mixed, "forwardAndClean");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.root.span.bytes() == b"x: WithText" && place.field().is_some_and(|decl| decl.name.as_deref() == Some("text")))
            );

            let caller = lower("callerForward");

            let mir = require_mir(&caller, "callerForward");
            assert!(
                mir.blocks
                    .iter()
                    .any(|block| matches!(block.operation, Operation::Call(_)))
            );
            assert!(
                path_drops(mir, mir.entry).is_empty(),
                "the call owns the transferred text"
            );
            let replacement = lower("fieldReplacement");

            let mir = require_mir(&replacement, "fieldReplacement");
            assert!(mir.blocks.iter().any(
                |block| matches!(block.operation, Operation::Install { target, .. }
                if target.field().is_some_and(|decl| decl.name.as_deref() == Some("value")))
            ));
            let ordered = lower("storageBeforeSource");

            let mir = require_mir(&ordered, "storageBeforeSource");
            assert!(mir.blocks.iter().any(|block| matches!(&block.operation, Operation::Cleanup(cleanup)
                if matches!(cleanup.as_slice(), [Obligation::EndStorage(field), Obligation::EndStorage(root), Obligation::EndStorage(source)]
                    if field.field().is_some_and(|decl| decl.name.as_deref() == Some("value"))
                        && root.projection.is_none() && root.root.span.bytes() == b"x"
                        && source.root.span.bytes() == b"local"))));

            let looped = lower("loopClears");

            let mir = require_mir(&looped, "loopClears");
            assert!(
                mir.blocks
                    .iter()
                    .enumerate()
                    .any(|(pc, b)| matches!(b.terminator, Terminator::Goto(next) if next < pc))
            );

            let grouped_reborrow = lower("groupedReborrow");
            let mir = require_mir(&grouped_reborrow, "groupedReborrow");
            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Reborrow { parent, exclusive: false, .. }
                if parent.field().is_some_and(|field| field.name.as_deref() == Some("value")))));

            let exclusive = lower("retainsExclusiveRestriction");
            assert_eq!(exclusive.completion(), Completion::SourceRejected);
            assert!(exclusive.mir().is_none());
            let [finding] = exclusive.findings() else {
                panic!("expected exclusive restriction")
            };
            assert_eq!(finding.code(), "R0201");
            assert_eq!(finding.related.span.bytes(), b"&mut local");

            let grouped = lower("groupedOnly");
            let mir = require_mir(&grouped, "groupedOnly");
            assert_eq!(grouped.completion(), Completion::Checked);
            assert!(grouped.findings().is_empty());
            let ops = linear(mir, mir.entry);
            let (backing, staged) = ops
                .iter()
                .find_map(|op| match op {
                    Operation::Borrow {
                        source,
                        holder,
                        exclusive: false,
                    } => Some((source, holder)),
                    _ => None,
                })
                .expect("borrow the sibling field");
            let stored = ops
                .iter()
                .find_map(|op| match op {
                    Operation::Install { target, incoming }
                        if incoming.root.node == staged.root.node =>
                    {
                        Some(target)
                    }
                    _ => None,
                })
                .expect("install the borrowed view");
            let owned = &backing.field().unwrap().id;
            let view = &stored.field().unwrap().id;
            assert_eq!(backing.field().unwrap().name.as_deref(), Some("owned"));
            assert_eq!(stored.field().unwrap().name.as_deref(), Some("view"));
            let x = backing.root.node;
            assert_eq!(backing.root.span.bytes(), b"x");
            assert_eq!(stored.root.node, x);
            let mut ends = Vec::new();
            for block in straight_blocks(mir, mir.entry) {
                let Operation::Cleanup(actions) = &block.operation else {
                    continue;
                };
                for action in actions {
                    if let Obligation::EndStorage(place) = action
                        && place.root.node == x
                    {
                        assert_eq!(block.site.span.bytes(), b"{\n                let mut x = LocalView { owned: true, view: seed };\n                x.view = &x.owned;\n            }");
                        assert!(place.projection.is_none() || place.field().is_some());
                        ends.push(place.field().map(|field| &field.id));
                    }
                }
            }
            assert_eq!(ends, [Some(view), Some(owned), None]);
            assert!(path_drops(mir, mir.entry).is_empty());

            let mixed = lower("mixedEdges");
            let mir = require_mir(&mixed, "mixedEdges");
            let ends: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter_map(|op| match op {
                    Operation::Cleanup(actions) => Some(actions.iter()),
                    _ => None,
                })
                .flatten()
                .filter_map(|action| match action {
                    Obligation::EndStorage(place) => Some((
                        place.root.span.bytes(),
                        place.field().map(|field| field.name.as_deref().unwrap()),
                    )),
                    _ => None,
                })
                .collect();
            // The later-declared backing must outlive the earlier holder, even
            // though the holder's own internal edge can use reverse field order.
            assert_eq!(
                ends,
                [
                    (b"holder".as_slice(), Some("external")),
                    (b"holder", Some("internal")),
                    (b"holder", Some("owned")),
                    (b"holder", None),
                    (b"backing", Some("view")),
                    (b"backing", Some("owned")),
                    (b"backing", None),
                    (b"seed: &bool", None),
                ]
            );
            assert!(path_drops(mir, mir.entry).is_empty());

            let limits = AnalysisLimits {
                max_work: grouped.statistics().work,
                max_entries: grouped.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower_with("groupedOnly", limits).mir().is_some());
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
                let limited = lower_with("groupedOnly", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

            for name in [
                "outOfOrder",
                "recordCycle",
                "observable",
                "protected",
                "replaceOwner",
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.findings().is_empty(), "{name}");
                assert!(report.mir().is_none(), "{name}");
                let site = report.blocked_at().unwrap().span.bytes();
                if name == "replaceOwner" {
                    assert_eq!(site, b"x = LocalView { owned: false, view: seed };");
                } else {
                    assert!(
                        site.starts_with(b"{\n                let mut x = "),
                        "{name}: {site:?}"
                    );
                }
            }
            let moved = lower("moveOwner");
            assert_eq!(moved.completion(), Completion::SourceRejected);
            assert!(moved.mir().is_none());
            let [finding] = moved.findings() else {
                panic!("expected the self-borrow to block the owner move")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"x");
            assert_eq!(finding.related.span.bytes(), b"&x.owned");

            let used = lower("usedCopy");
            let mir = require_mir(&used, "usedCopy");
            let ops = linear(mir, mir.entry);
            let staged = ops
                .iter()
                .find_map(|op| match op {
                    Operation::Borrow {
                        source,
                        holder,
                        exclusive: false,
                    } if source.root.span.bytes() == b"x"
                        && source
                            .field()
                            .is_some_and(|f| f.name.as_deref() == Some("owned")) =>
                    {
                        Some(holder)
                    }
                    _ => None,
                })
                .expect("borrow the owned field, not the whole record");
            assert!(
                ops.iter()
                    .any(|op| matches!(op, Operation::Install { target, incoming }
                if target.root.span.bytes() == b"x"
                    && target.field().is_some_and(|f| f.name.as_deref() == Some("view"))
                    && incoming.root.node == staged.root.node))
            );
            assert!(
                ops.iter()
                    .any(|op| matches!(op, Operation::Copy { from, into: Some(to) }
                if from.root.span.bytes() == b"x"
                    && from.field().is_some_and(|f| f.name.as_deref() == Some("view"))
                    && to.root.span.bytes() == b"copied"))
            );

            let last_use = ops
                .iter()
                .position(|op| {
                    matches!(op, Operation::Dereference(place)
                if place.root.span.bytes() == b"copied")
                })
                .expect("use the copied view");
            let replace = ops
                .iter()
                .position(|op| {
                    matches!(op, Operation::Install { target, .. }
                if target.field().is_some_and(|field| &field.id == owned))
                })
                .expect("replace the backing field");
            assert!(last_use < replace);

            for (name, site) in [
                ("liveCopy", b"x.owned = false;".as_slice()),
                ("liveAcrossBoundary", b"{\n                    let mut x = LocalView { owned: true, view: seed };\n                    x.view = &x.owned;\n                    copied = x.view;\n                }"),
            ] {
                let live = lower(name);
                assert_eq!(live.completion(), Completion::SourceRejected, "{name}");
                assert!(live.mir().is_none(), "{name}");
                let [finding] = live.findings() else {
                    panic!("{name}: expected the copied view to block release of its backing");
                };
                assert_eq!(finding.code(), "R0203", "{name}");
                assert_eq!(finding.site.span.bytes(), site, "{name}");
                assert_eq!(finding.related.span.bytes(), b"&x.owned", "{name}");
            }

            for name in [
                "callerRetains",
                "otherHolder",
                "possibleSource",
                "futureProjection",
                "reborrowAfterRelease",
                "borrowHolder",
                "branchRetains",
                "loopEscapes",
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none(), "{name}");
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected release finding")
                };
                assert_eq!(finding.code(), "R0203", "{name}");
                assert_eq!(finding.related.span.bytes(), b"&local", "{name}");
            }

            for (name, source) in [
                ("escapes", b"&x.value".as_slice()),
                ("escapeLocal", b"local".as_slice()),
                ("escapeInRecord", b"local".as_slice()),
                ("callerEscapes", b"local".as_slice()),
                ("reborrowEscape", b"local".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none(), "{name}");
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected an escape finding")
                };
                assert_eq!(finding.code(), "R0202", "{name}");
                assert_eq!(finding.related.span.bytes(), source, "{name}");
            }
        },
    );
}
