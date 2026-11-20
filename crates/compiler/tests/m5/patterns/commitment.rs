use super::*;
use compiler::analysis::{DeclarationShape, ownership::analyze_ownership, scalars::BodyOwner};

#[test]
fn interrupted_record_commits_binders_only_on_success() {
    probe_with_typed(
        "patterns::commitment::interrupted_record_commits_binders_only_on_success",
        r#"
        type Pack = { first: string; check: bool; second: string; last: string; }
        function pass(value: string): string { return value; }
        function event(): void {}
        function directInterrupt(a: string, b: string, c: string, spare: string, gate: Option<bool>): Option<bool> {
            let kept = spare;
            let Pack { first: selected, check: bit, last: _ } = Pack { second: pass(c), first: a, check: gate?, last: pass(b) };
            event(); let tail = selected; return Option.Some(bit);
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
                let (commit, stage) = success
                    .iter()
                    .enumerate()
                    .find_map(|(i, b)| match b.operation {
                        Operation::Initialize(p) if p.root.span.bytes().starts_with(b"Pack {") => {
                            Some((i, p.root.node))
                        }
                        _ => None,
                    })
                    .unwrap();
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
                assert!(success[*event + 1..].windows(2).any(|pair| matches!(
                    (&pair[0].operation, &pair[1].operation),
                    (Operation::ConstructVariant {variant, into, ..}, Operation::Export(out))
                        if *variant == &role.some.id && into.root.node == out.root.node)));
                assert!(
                    matches!(path_drops(mir,yes).as_slice(),[Obligation::Drop(last),Obligation::Drop(second),Obligation::Drop(tail),Obligation::Drop(kept)] if last.root.node==stage && second.root.node==stage && tail.root.span.bytes()==b"tail" && kept.root.span.bytes()==b"kept")
                );
            }
            let skipped = run("skipMovedSuffix", default);
            let mir = skipped.mir().unwrap();
            assert!(root("skipMovedSuffix").ordinary_mir().is_some());
            assert!(!mir.blocks.iter().any(|b| b.site.span.bytes() == b"pass(b)"
                || matches!(b.operation,Operation::Call(id) if id==&decl("event").id)));
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
