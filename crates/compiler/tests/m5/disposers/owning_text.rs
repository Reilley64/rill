//! A custom disposer over a record whose fields own `string` values
//! (ri-px1.7, row D2), on the normal edge and under explicit `dispose`.
//!
//! The caller transfers the whole record to its selected outer disposer once.
//! The disposer body runs, then a compiler-only epilogue drops only the fields
//! still initialized, in reverse declaration order: a field moved in the body
//! is skipped. Each case is checked by the targeted query and by its ordinary
//! graph root, in both installed load modes. These are compiler judgments, not
//! Rill execution.
use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{
    DependencyContract, GraphCompletion, GraphReport, Place, Report, RootReport, analyze_ownership,
};

const SOURCE: &str = r#"
            import std::core::dispose as release;
            type Note = { head: string; moved: string; tail: string; }
            implement release(value: Note): void {
                let saved = value.moved;
            }
            function normal(value: Note): void { }
            function explicit(value: Note, later: string): void { release(value); }
            function canonical(value: Note): void { dispose(value); }
            function outside(value: Note): string { return value.moved; }
            function outsideLocal(value: Note): void { let taken = value.head; }
            type Reread = { text: string; other: string; }
            implement release(value: Reread): void {
                let taken = value.text;
                let again = value.text;
            }
            function reread(value: Reread): void { }
            type Manual = { first: string; second: string; }
            implement release(value: Manual): void { dispose(value.second); }
            function manual(value: Manual): void { }
"#;

fn stats(work: usize, entries: usize, max_depth: usize) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

fn caps(s: Statistics) -> (AnalysisLimits, [AnalysisLimits; 3]) {
    let exact = AnalysisLimits {
        max_work: s.work,
        max_entries: s.entries,
        max_depth: s.max_depth,
    };
    (
        exact,
        [
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
        ],
    )
}

fn named<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
}

fn graph_root<'a, 'g>(
    graph: &'a GraphReport<'g>,
    module: &'g loader::Module,
    name: &str,
) -> &'a RootReport<'g> {
    let node = named(module, name).node;
    graph
        .roots()
        .iter()
        .find(|r| r.site().module == &module.key && r.site().node == node)
        .unwrap()
}

/// The reached active body of the disposer whose header starts with `header`.
fn active_body<'a, 'g>(graph: &'a GraphReport<'g>, header: &str) -> &'a RootReport<'g> {
    let [body] = graph
        .active_disposers()
        .iter()
        .filter(|r| r.site().span.bytes().starts_with(header.as_bytes()))
        .collect::<Vec<_>>()[..]
    else {
        panic!("expected one active body for {header}");
    };
    body
}

/// A place as written. The compiler-only epilogue that receives a disposer's
/// remaining fields has no source name; it is spelled `epilogue`.
fn text(place: &Place<'_>, epilogue: Option<usize>) -> String {
    let root = if Some(place.root.node) == epilogue {
        "epilogue".to_string()
    } else {
        String::from_utf8_lossy(place.root.span.bytes()).into_owned()
    };
    match place.projection {
        None => root,
        Some(_) => format!("{root}.{}", place.field().unwrap().name.as_deref().unwrap()),
    }
}

fn kind(value: &impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    debug[..debug.find([' ', '(', '{']).unwrap_or(debug.len())].to_string()
}

/// The whole ordered action list of a straight path, one `(kind, place)` per
/// action, with no filtering.
fn sequence(mir: &Mir<'_>) -> Vec<(&'static str, String)> {
    let blocks = straight_blocks(mir, mir.entry);
    assert!(matches!(
        blocks.last().unwrap().terminator,
        Terminator::Return
    ));
    let mut epilogue = None;
    let mut events = Vec::new();
    for block in blocks {
        match &block.operation {
            Operation::None => {}
            Operation::Initialize(place) => events.push(("init", text(place, epilogue))),
            Operation::Copy { from, into } | Operation::Move { from, into } => events.push((
                if matches!(block.operation, Operation::Copy { .. }) {
                    "copy"
                } else {
                    "move"
                },
                match into {
                    Some(to) => format!("{} -> {}", text(from, epilogue), text(to, epilogue)),
                    None => text(from, epilogue),
                },
            )),
            Operation::Export(place) => events.push(("export", text(place, epilogue))),
            Operation::Dispose { operand, .. } => events.push(("dispose", text(operand, epilogue))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    if let Obligation::DeferDisposerFields { into, .. } = action {
                        epilogue = Some(into.root.node);
                    }
                    events.push(match action {
                        Obligation::EndStorage(place) => ("end", text(place, epilogue)),
                        Obligation::Drop(place) => ("drop", text(place, epilogue)),
                        Obligation::InvokeDisposer { operand, .. } => {
                            ("disposer", text(operand, epilogue))
                        }
                        Obligation::DeferDisposerFields { from, into } => (
                            "defer",
                            format!("{} -> {}", text(from, epilogue), text(into, epilogue)),
                        ),
                        other => panic!("unexpected cleanup {}", kind(other)),
                    });
                }
            }
            other => panic!("unexpected operation {}", kind(other)),
        }
    }
    events
}

fn owned_events(events: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    events.iter().map(|(k, p)| (*k, p.to_string())).collect()
}

/// Exact replay of `pinned`, then each independently one-short cap.
fn assert_targeted<'g>(
    query: impl Fn(AnalysisLimits) -> Report<'g>,
    pinned: Statistics,
    completion: Completion,
) {
    let (exact, short) = caps(pinned);
    let replay = query(exact);
    assert_eq!(replay.completion(), completion);
    assert_eq!(replay.statistics(), pinned);
    for short in short {
        let limited = query(short);
        assert_eq!(limited.completion(), Completion::ResourceLimit);
        assert!(limited.mir().is_none() && limited.findings().is_empty());
    }
}

fn assert_finding(
    finding: &compiler::analysis::ownership::Finding<'_>,
    expected: (&str, &str, &str),
) {
    let (code, site, related) = expected;
    assert_eq!(finding.code(), code);
    assert_eq!(finding.site.span.bytes(), site.as_bytes());
    assert_eq!(finding.related.span.bytes(), related.as_bytes());
}

/// Normal edge, explicit disposal through the alias, and explicit canonical
/// `dispose`: the caller invokes the outer disposer once and drops no field.
/// The body moves `moved` out; the epilogue drops only `tail`, then `head`.
#[test]
fn outer_disposer_runs_once_then_drops_the_remaining_text_fields() {
    probe(
        "disposers::owning_text::outer_disposer_runs_once_then_drops_the_remaining_text_fields",
        SOURCE,
        |_, module, receipt| {
            let query = |name: &str| {
                let id = &named(module, name).id;
                move |limits| lower_body(receipt, module, id, limits).unwrap()
            };
            let graph = analyze_ownership(receipt, AnalysisLimits::default());

            // One body for every caller: `moved` goes to `saved`, which drops
            // at the body's end. The remaining fields pass to the epilogue,
            // which drops `tail` then `head` once. `moved` is not dropped again.
            let body = owned_events(&[
                ("move", "value: Note.moved -> saved"),
                ("drop", "saved"),
                ("defer", "value: Note -> epilogue"),
                ("drop", "epilogue.tail"),
                ("drop", "epilogue.head"),
                ("end", "epilogue"),
            ]);
            let active = active_body(&graph, "implement release(value: Note)");
            assert_eq!(active.ordinary_completion(), Completion::Checked);
            assert_eq!(sequence(active.ordinary_mir().unwrap()), body);
            assert!(active.dependencies().is_empty());

            for (name, pinned, caller) in [
                (
                    "normal",
                    stats(5_552, 542, 3),
                    &[("disposer", "value: Note")][..],
                ),
                (
                    "explicit",
                    stats(5_701, 622, 3),
                    &[
                        ("disposer", "value: Note"),
                        ("init", "release(value)"),
                        ("drop", "later: string"),
                    ][..],
                ),
                (
                    "canonical",
                    stats(5_664, 604, 3),
                    &[("disposer", "value: Note"), ("init", "dispose(value)")][..],
                ),
            ] {
                let target = query(name)(AnalysisLimits::default());
                assert_eq!(target.completion(), Completion::Checked, "{name}");
                assert!(target.findings().is_empty());
                assert_eq!(target.statistics(), pinned, "{name}");
                let mir = target.mir().unwrap();
                let caller = owned_events(caller);
                assert_eq!(sequence(mir), caller, "{name}");
                let [implementation] = mir
                    .blocks
                    .iter()
                    .flat_map(|block| match &block.operation {
                        Operation::Cleanup(actions) => actions.as_slice(),
                        _ => &[],
                    })
                    .filter_map(|action| match action {
                        Obligation::InvokeDisposer { implementation, .. } => Some(*implementation),
                        _ => None,
                    })
                    .collect::<Vec<_>>()[..]
                else {
                    panic!("{name}: expected one outer disposer invocation");
                };
                let selected = module
                    .declarations
                    .iter()
                    .find(|d| &d.id == implementation)
                    .unwrap();
                assert!(
                    selected
                        .span
                        .bytes()
                        .starts_with(b"implement release(value: Note)")
                );
                assert_eq!(
                    sequence(mir.disposer(implementation).unwrap()),
                    body,
                    "{name}"
                );
                assert_targeted(query(name), pinned, Completion::Checked);

                // The ordinary graph root checks with the same MIR; its one
                // edge is the reached active disposer above.
                let root = graph_root(&graph, module, name);
                assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                assert_eq!(sequence(root.ordinary_mir().unwrap()), caller, "{name}");
                assert!(matches!(root.dependencies(), [edge]
                    if edge.contract == DependencyContract::ActiveDisposer
                        && edge.declaration == implementation));
            }

            // The whole receipt for this source.
            let pinned = stats(197_595, 5_829, 3);
            assert_eq!(graph.statistics(), pinned);
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            let (exact, short) = caps(pinned);
            let replay = analyze_ownership(receipt, exact);
            assert_eq!(replay.statistics(), pinned);
            assert_eq!(replay.completion(), GraphCompletion::SourceRejected);
            for short in short {
                let limited = analyze_ownership(receipt, short);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.roots().iter().all(|root| root.mir().is_none()));
            }
        },
    );
}

/// Rereading a field the active body moved gives `R0200`; moving a field
/// out of the record outside its active disposer gives `R0501`. None
/// publishes MIR, in the targeted query or at the ordinary graph root.
#[test]
fn text_fields_reject_reread_and_moves_outside_the_active_disposer() {
    probe(
        "disposers::owning_text::text_fields_reject_reread_and_moves_outside_the_active_disposer",
        SOURCE,
        |_, module, receipt| {
            let query = |name: &str| {
                let id = &named(module, name).id;
                move |limits| lower_body(receipt, module, id, limits).unwrap()
            };
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let rejected = [
                (
                    "outside",
                    stats(2_428, 179, 3),
                    ("R0501", "value.moved", "value: Note"),
                ),
                (
                    "outsideLocal",
                    stats(2_438, 188, 3),
                    ("R0501", "value.head", "value: Note"),
                ),
                (
                    "reread",
                    stats(5_369, 470, 3),
                    ("R0200", "value.text", "value.text"),
                ),
            ];
            for (name, pinned, expected) in rejected {
                let target = query(name)(AnalysisLimits::default());
                assert_eq!(target.completion(), Completion::SourceRejected, "{name}");
                assert!(target.mir().is_none());
                assert_eq!(target.statistics(), pinned, "{name}");
                let [finding] = target.findings() else {
                    panic!("{name}: expected one finding");
                };
                assert_finding(finding, expected);
                assert_targeted(query(name), pinned, Completion::SourceRejected);

                let root = graph_root(&graph, module, name);
                assert_eq!(
                    root.ordinary_completion(),
                    Completion::SourceRejected,
                    "{name}"
                );
                assert!(root.ordinary_mir().is_none());
            }
            // Each witness is recorded once, the reread in the active body.
            let [outside, local, reread] = graph.findings() else {
                panic!("expected one finding per rejected body");
            };
            for (finding, (_, _, expected)) in [outside, local, reread].into_iter().zip(rejected) {
                assert_finding(finding, expected);
            }
            assert!(reread.related.span.start() < reread.site.span.start());
            let active = active_body(&graph, "implement release(value: Reread)");
            assert_eq!(active.ordinary_completion(), Completion::SourceRejected);
            assert!(active.ordinary_mir().is_none());
        },
    );
}

/// The boundary of this row: a body that writes `dispose(value.second)` on a
/// field checks in the targeted query, but a written disposal of a field place
/// is not a verified graph edge, so the ordinary graph root stays `Incomplete`.
#[test]
fn a_written_field_disposal_keeps_the_graph_root_open() {
    probe(
        "disposers::owning_text::a_written_field_disposal_keeps_the_graph_root_open",
        SOURCE,
        |_, module, receipt| {
            let id = &named(module, "manual").id;
            let query = |limits| lower_body(receipt, module, id, limits).unwrap();
            let pinned = stats(5_487, 518, 3);
            let target = query(AnalysisLimits::default());
            assert_eq!(target.completion(), Completion::Checked);
            assert_eq!(target.statistics(), pinned);
            let mir = target.mir().unwrap();
            assert_eq!(
                sequence(mir),
                owned_events(&[("disposer", "value: Manual")])
            );
            let [Obligation::InvokeDisposer { implementation, .. }] =
                path_drops(mir, mir.entry)[..]
            else {
                panic!("expected one outer disposer invocation");
            };
            assert_eq!(
                sequence(mir.disposer(implementation).unwrap()),
                owned_events(&[
                    ("dispose", "value: Manual.second"),
                    ("init", "dispose(value.second)"),
                    ("defer", "value: Manual -> epilogue"),
                    ("drop", "epilogue.first"),
                    ("end", "epilogue"),
                ])
            );
            assert_targeted(query, pinned, Completion::Checked);

            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let root = graph_root(&graph, module, "manual");
            assert_eq!(root.ordinary_completion(), Completion::Incomplete);
            assert!(root.ordinary_mir().is_none());
            assert_eq!(
                root.blocked_at().unwrap().span.bytes(),
                b"dispose(value.second)"
            );
            let active = active_body(&graph, "implement release(value: Manual)");
            assert_eq!(active.ordinary_completion(), Completion::Incomplete);
            assert!(matches!(active.dependencies(), [edge]
                if edge.contract == DependencyContract::BuiltinDispose));
        },
    );
}
