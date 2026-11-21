use super::*;
use compiler::analysis::{DeclarationShape, ownership::analyze_ownership, scalars::BodyOwner};

#[test]
fn grouped_record_fields_keep_the_named_initializer_budget() {
    probe(
        "patterns::commitment::grouped_record_fields_keep_the_named_initializer_budget",
        r#"
        type Flags = { first: bool; second: bool; }
        function supported(input: Option<bool>): Option<bool> {
            let owner = Flags { first: (input?), second: true }; return Option.Some(owner.first);
        }
        function unsupported(input: Option<bool>): Option<bool> {
            let owner = Flags { first: input?, second: ((((((((((input?)))))))))) }; return Option.Some(owner.first);
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
            // Exact ff8f725 work and entry caps; grouping does not add a second terminal visit.
            let good = run(
                "supported",
                AnalysisLimits {
                    max_work: 4187,
                    max_entries: 720,
                    ..Default::default()
                },
            );
            assert!(good.mir().is_some());
            assert_eq!(
                run("unsupported", AnalysisLimits::default()).completion(),
                Completion::Incomplete
            );
            let short = run(
                "unsupported",
                AnalysisLimits {
                    max_depth: good.statistics().max_depth,
                    ..Default::default()
                },
            );
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
        },
    );
}

#[test]
fn interrupted_record_commits_binders_only_on_success() {
    probe_with_typed(
        "patterns::commitment::interrupted_record_commits_binders_only_on_success",
        r#"
        type Pack = { first: string; check: bool; second: string; last: string; }
        function pass(value: string): string { return value; }
        function event(): void {}
        function directTotal(a: string, b: string, c: string, spare: string, gate: bool): Option<bool> {
            let kept = spare;
            let Pack { first: selected, check: bit, last: _ } = Pack { second: pass(c), first: a, check: gate, last: pass(b) };
            event(); let tail = selected; return Option.Some(bit);
        }
        function splitInterrupt(a: string, b: string, c: string, spare: string, gate: Option<bool>): Option<bool> {
            let kept = spare;
            let owner = Pack { second: pass(c), first: a, check: gate?, last: pass(b) };
            let Pack { first: selected, check: bit, last: _ } = owner;
            event(); let tail = selected; return Option.Some(bit);
        }
        function directInterrupt(a: string, b: string, c: string, spare: string, gate: Option<bool>): Option<bool> {
            let kept = spare;
            let Pack { first: selected, check: bit, last: _ } = Pack { second: pass(c), first: a, check: gate?, last: pass(b) };
            event(); let tail = selected; return Option.Some(bit);
        }
        function grouped(a: string, b: string, c: string, spare: string, gate: Option<bool>): Option<bool> {
            let kept = spare;
            let Pack { first: selected, check: bit, last: _ } = (((Pack { second: pass(c), first: a, check: (((((((gate?))))))), last: pass(b) })));
            event(); let tail = selected; return Option.Some(bit);
        }
        function afterCommit(a: string, b: string, c: string, spare: string, gate: Option<bool>): Option<bool> {
            let kept = spare;
            let Pack { first: selected, last: _ } = Pack { second: pass(c), first: a, check: true, last: pass(b) };
            let bit = gate?; event(); let tail = selected; return Option.Some(bit);
        }
        function skipMovedSuffix(a: string, b: string, c: string): Option<bool> {
            let prior = b;
            let Pack { first: selected, check: bit, last: _ } = Pack { second: pass(c), first: a, check: Option<bool>.None?, last: pass(b) };
            event(); return Option.Some(bit);
        }
        function reachMovedSuffix(a: string, b: string, c: string): Option<bool> {
            let prior = b;
            let Pack { first: selected, check: bit, last: _ } = Pack { second: pass(c), first: a, check: Option<bool>.Some(true)?, last: pass(b) };
            event(); return Option.Some(bit);
        }
        function movedPrefix(a: string, b: string, c: string, gate: Option<bool>): Option<bool> {
            let prior = a;
            let Pack { first: selected, check: bit, last: _ } = Pack { second: pass(c), first: a, check: gate?, last: pass(b) };
            event(); return Option.Some(bit);
        }
    "#,
        |_, module, receipt, typed| {
            let decl = |name: &str| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let run = |name, limits| lower_body(receipt, module, &decl(name).id, limits).unwrap();
            let default = AnalysisLimits::default();
            // Exact pre-change budgets measured at ff8f725 for this capture.
            for (name, max_work, max_entries) in [
                ("directTotal", 10759, 2216),
                ("splitInterrupt", 13171, 3297),
            ] {
                assert!(
                    run(
                        name,
                        AnalysisLimits {
                            max_work,
                            max_entries,
                            ..default
                        }
                    )
                    .mir()
                    .is_some(),
                    "{name}"
                );
            }
            let graph = analyze_ownership(receipt, default);
            let root = |name| {
                graph
                    .roots()
                    .iter()
                    .find(|r| r.owner() == BodyOwner::Declaration(&decl(name).id))
                    .unwrap()
            };
            let role = typed.option_role().unwrap();
            let pack = typed
                .inventory()
                .type_facts()
                .declarations()
                .find(|f| f.declaration.id == decl("Pack").id)
                .unwrap();
            let DeclarationShape::Record { fields } = &pack.shape else {
                panic!("record layout")
            };
            let field = |name| {
                &fields
                    .iter()
                    .find(|f| f.declaration.name.as_deref() == Some(name))
                    .unwrap()
                    .declaration
                    .id
            };
            let direct = run("directInterrupt", default);
            assert_eq!(
                direct.completion(),
                Completion::Checked,
                "{:?}",
                direct
                    .blocked_at()
                    .map(|s| s.span.bytes().escape_ascii().to_string())
            );
            for mir in [
                direct.mir().unwrap(),
                root("directInterrupt").ordinary_mir().unwrap(),
            ] {
                let prefix = straight_blocks(mir, mir.entry);
                let split = prefix.last().unwrap();
                assert!(
                    matches!(split.operation, Operation::TestVariant{success,..} if success==&role.some.id)
                );
                let Terminator::Branch { yes, no } = split.terminator else {
                    panic!("expected both gate exits")
                };
                let prefix_calls: Vec<_> = prefix
                    .iter()
                    .filter_map(|b| match b.operation {
                        Operation::Call(id) => Some((id, b.site.span.bytes())),
                        _ => None,
                    })
                    .collect();
                assert_eq!(prefix_calls, [(&decl("pass").id, b"pass(c)".as_slice())]);
                let committed = |op: &Operation<'_>| match op {
                    Operation::Initialize(p) => {
                        p.projection.is_none() && p.root.span.bytes().starts_with(b"Pack {")
                    }
                    Operation::Move { into: Some(p), .. }
                    | Operation::Copy { into: Some(p), .. } => {
                        matches!(p.root.span.bytes(), b"selected" | b"bit")
                    }
                    _ => false,
                };
                assert!(
                    prefix.iter().all(|b| !committed(&b.operation)),
                    "uncommitted prefix"
                );
                let failure: Vec<_> = linear(mir, no)
                    .into_iter()
                    .filter(|op| !matches!(op, Operation::None))
                    .collect();
                let [
                    Operation::ConstructVariant {
                        variant,
                        payload: None,
                        into,
                    },
                    Operation::Export(out),
                    Operation::Drop(first),
                    Operation::Drop(second),
                    Operation::Cleanup(locals),
                    Operation::Cleanup(params),
                ] = failure.as_slice()
                else {
                    panic!(
                        "expected only None export, initialized prefix drops, and lexical cleanup"
                    );
                };
                assert_eq!(*variant, &role.none.id);
                assert_eq!(into.root.node, out.root.node);
                assert_eq!(&first.field().unwrap().id, field("first"));
                assert_eq!(&second.field().unwrap().id, field("second"));
                assert_eq!(first.root.node, second.root.node);
                assert!(
                    matches!(locals.as_slice(), [Obligation::Drop(kept)] if kept.root.span.bytes() == b"kept")
                );
                assert!(
                    matches!(params.as_slice(), [Obligation::EndStorage(gate), Obligation::Drop(b)]
                    if gate.root.span.bytes() == b"gate: Option<bool>" && b.root.span.bytes() == b"b: string")
                );
                let success = straight_blocks(mir, yes);
                let calls: Vec<_> = success
                    .iter()
                    .enumerate()
                    .filter_map(|(i, b)| match b.operation {
                        Operation::Call(id) => Some((i, id, b.site.span.bytes())),
                        _ => None,
                    })
                    .collect();
                let [(suffix, pass, site), (event, notify, _)] = calls.as_slice() else {
                    panic!("expected suffix and following statement")
                };
                assert_eq!(
                    (*pass, *site, *notify),
                    (&decl("pass").id, b"pass(b)".as_slice(), &decl("event").id)
                );
                let headers: Vec<_> = success
                    .iter()
                    .enumerate()
                    .filter_map(|(i, b)| match b.operation {
                        Operation::Initialize(p)
                            if p.projection.is_none()
                                && p.root.span.bytes().starts_with(b"Pack {") =>
                        {
                            Some((i, p.root.node))
                        }
                        _ => None,
                    })
                    .collect();
                let [(commit, stage)] = headers[..] else {
                    panic!("expected one complete header only on success")
                };
                assert_eq!(
                    stage, first.root.node,
                    "both edges use the one constructor stage"
                );
                let staged_second = prefix
                    .iter()
                    .position(
                        |b| matches!(b.operation, Operation::Call(id) if id == &decl("pass").id),
                    )
                    .unwrap();
                let staged_first = prefix.iter().position(|b| matches!(b.operation,
                    Operation::Move { from, into: Some(to) } if from.root.span.bytes() == b"a: string"
                        && to.root.node == stage && to.field().map(|d| &d.id) == Some(field("first")))).unwrap();
                assert!(staged_second < staged_first);
                let binder = |name, tag, move_| {
                    success
                        .iter()
                        .position(|b| match b.operation {
                            Operation::Move {
                                from,
                                into: Some(into),
                            } if move_ => {
                                from.root.node == stage
                                    && from.field().map(|d| &d.id) == Some(tag)
                                    && into.root.span.bytes() == name
                            }
                            Operation::Copy {
                                from,
                                into: Some(into),
                            } if !move_ => {
                                from.root.node == stage
                                    && from.field().map(|d| &d.id) == Some(tag)
                                    && into.root.span.bytes() == name
                            }
                            _ => false,
                        })
                        .unwrap()
                };
                let m = binder(b"selected".as_slice(), field("first"), true);
                let Operation::Move {
                    into: Some(selected),
                    ..
                } = success[m].operation
                else {
                    unreachable!()
                };
                let c = binder(b"bit".as_slice(), field("check"), false);
                let remainder: Vec<_> = success
                    .iter()
                    .enumerate()
                    .flat_map(|(i, b)| {
                        match &b.operation {
                            Operation::Cleanup(actions) => actions.as_slice(),
                            _ => &[],
                        }
                        .iter()
                        .filter_map(move |a| match a {
                            Obligation::Drop(p) if p.root.node == stage => {
                                Some((i, &p.field().unwrap().id))
                            }
                            _ => None,
                        })
                    })
                    .collect();
                let [(i, last), (j, second)] = remainder.as_slice() else {
                    panic!("expected only two unbound owners")
                };
                assert_eq!((*last, *second), (field("last"), field("second")));
                assert!(*suffix < commit && commit < m && m < c && c < *i && i <= j && *j < *event);
                let (moved, tail_node) = success
                    .iter()
                    .enumerate()
                    .find_map(|(i, b)| match b.operation {
                        Operation::Move {
                            from,
                            into: Some(to),
                        } if from.root.node == selected.root.node
                            && to.root.span.bytes() == b"tail" =>
                        {
                            Some((i, to.root.node))
                        }
                        _ => None,
                    })
                    .unwrap();
                let exported = success
                    .windows(2)
                    .position(|pair| {
                        matches!(
                    (&pair[0].operation, &pair[1].operation),
                    (Operation::ConstructVariant {variant, into, ..}, Operation::Export(out))
                        if *variant == &role.some.id && into.root.node == out.root.node)
                    })
                    .unwrap()
                    + 1;
                assert!(*event < moved && moved < exported);
                assert!(success[exported+1..].iter().any(|b| matches!(&b.operation,
                    Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action, Obligation::Drop(p) if p.root.node == tail_node)))));
                assert!(
                    matches!(path_drops(mir,yes).as_slice(),[Obligation::Drop(last),Obligation::Drop(second),Obligation::Drop(tail),Obligation::Drop(kept)] if last.root.node==stage && second.root.node==stage && tail.root.span.bytes()==b"tail" && kept.root.span.bytes()==b"kept")
                );
            }
            let skipped = run("skipMovedSuffix", default);
            let mir = skipped.mir().unwrap();
            assert!(root("skipMovedSuffix").ordinary_mir().is_some());
            assert!(!mir.blocks.iter().any(|b| b.site.span.bytes() == b"pass(b)"
                || matches!(b.operation,Operation::Call(id) if id==&decl("event").id)));
            assert!(mir.blocks.iter().all(|b| match &b.operation {
                Operation::Initialize(p) => p.projection.is_some() || !p.root.span.bytes().starts_with(b"Pack {"),
                Operation::Move {into:Some(p),..} | Operation::Copy {into:Some(p),..} => !matches!(p.root.span.bytes(), b"selected" | b"bit"),
                Operation::Cleanup(actions) => !actions.iter().any(|a| matches!(a, Obligation::EndStorage(p)|Obligation::Drop(p) if matches!(p.root.span.bytes(), b"selected" | b"bit") || p.root.span.bytes().starts_with(b"Pack {"))),
                _=>true,
            }));
            assert!(
                matches!(path_drops(mir,mir.entry).as_slice(), [Obligation::Drop(first),Obligation::Drop(second),Obligation::Drop(prior)] if first.field().map(|d|&d.id)==Some(field("first")) && second.field().map(|d|&d.id)==Some(field("second")) && prior.root.span.bytes()==b"prior")
            );
            let after = run("afterCommit", default);
            let attached = after.mir().expect("postfix after a complete binding");
            let (_, failure) = branch_targets(attached);
            assert!(
                matches!(path_drops(attached, failure).as_slice(), [Obligation::Drop(selected), Obligation::Drop(kept)] if selected.root.span.bytes()==b"selected" && kept.root.span.bytes()==b"kept")
            );
            let grouped = run("grouped", default);
            assert!(grouped.mir().is_some());
            let grouped_depth = AnalysisLimits {
                max_depth: direct.statistics().max_depth,
                ..default
            };
            let short = run("grouped", grouped_depth);
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
            for (name, read, prior) in [
                (
                    "reachMovedSuffix",
                    b"b".as_slice(),
                    b"let prior = b;".as_slice(),
                ),
                ("movedPrefix", b"a", b"let prior = a;"),
            ] {
                let r = run(name, default);
                assert_eq!(r.completion(), Completion::SourceRejected);
                assert!(r.mir().is_none() && root(name).mir().is_none());
                let [f] = r.findings() else {
                    panic!("expected moved source")
                };
                assert_eq!(f.code(), "R0200");
                assert_eq!(f.site.span.bytes(), read);
                assert_eq!(f.related.span.bytes(), read);
                let body = &decl(name).span;
                let at = body.start()
                    + body
                        .bytes()
                        .windows(prior.len())
                        .position(|w| w == prior)
                        .unwrap()
                    + b"let prior = ".len();
                assert_eq!(f.related.span.start(), at);
                let (context, offset): (&[u8], usize) = if name == "movedPrefix" {
                    (b"first: a, check:", b"first: ".len())
                } else {
                    (b"last: pass(b)", b"last: pass(".len())
                };
                let use_at = body
                    .bytes()
                    .windows(context.len())
                    .position(|w| w == context)
                    .unwrap();
                assert_eq!(f.site.span.start(), body.start() + use_at + offset);
                assert!(graph.findings().iter().any(|g| g.code() == f.code()
                    && g.site.span == f.site.span
                    && g.related.span == f.related.span));
            }
            let short_graph = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_entries: graph.statistics().entries - 1,
                    ..default
                },
            );
            assert_eq!(
                short_graph.completion(),
                compiler::analysis::ownership::GraphCompletion::ResourceLimit
            );
            assert!(short_graph.roots().iter().all(|r| r.mir().is_none()));
            let s = direct.statistics();
            let exact = AnalysisLimits {
                max_work: s.work,
                max_entries: s.entries,
                max_depth: s.max_depth,
            };
            assert!(run("directInterrupt", exact).mir().is_some());
            for lim in [
                AnalysisLimits {
                    max_work: s.work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: s.entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: s.max_depth - 1,
                    ..exact
                },
            ] {
                let r = run("directInterrupt", lim);
                assert_eq!(r.completion(), Completion::ResourceLimit);
                assert!(r.mir().is_none() && r.findings().is_empty());
            }
        },
    );
}
