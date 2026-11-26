//! Consuming `for` over a List literal of string-field records (ri-vgz.7,
//! gap-analysis row C5): after `dispose(item.take)`, `break` cleans only the
//! current item's `keep`, then the unyielded whole records.
use super::*;
use compiler::analysis::DeclarationShape;
use compiler::analysis::ownership::{RecordTextDisposal, TextElementDisposal};
use compiler::analysis::typed::{Primitive, PrimitiveCoverage, PrimitiveOperation, PrimitiveRole};
use rill_collection::List;

const SOURCE: &str = r#"
import std::core::dispose as release;
type Entry = { take: string; keep: string; }
type Mixed = { take: string; count: i32; }
type Custom = { take: string; keep: string; }
implement release(value: Custom): void { dispose(value.keep); dispose(value.take); }
function onBreak(a: string, b: string, c: string, d: string): void {
    for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
        dispose(item.take);
        break;
    }
}
function onPostfix(a: string, b: string, c: string, d: string, gate: Option<bool>): Option<bool> {
    for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
        dispose(item.take);
        let success = gate?;
        return Option.Some(success);
    }
    return Option.Some(false);
}
function grouped(a: string, b: string, c: string, d: string): void {
    for (let (item) of [Entry{take: a, keep: b}, (Entry{take: c, keep: d})]) {
        dispose((item.take));
        break;
    }
}
function dead(a: string, b: string, c: string, d: string): void {
    return;
    for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
        break;
    }
}
function pruned(a: string, b: string, c: string, d: string): void {
    if (false) {
        for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
            break;
        }
    }
}
function scoped(a: string, b: string, c: string, d: string): void {
    {
        for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
            dispose(item.take);
            break;
        }
    }
}
function wrapped(flag: bool, a: string, b: string, c: string, d: string): void {
    while (flag) {
        for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
            dispose(item.take);
            break;
        }
        break;
    }
}
function whole(a: string, b: string, c: string, d: string): void {
    for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
        break;
    }
}
function exhausted(a: string, b: string, c: string, d: string): void {
    for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
        dispose(item.keep);
    }
}
function reused(a: string, b: string, c: string, d: string): void {
    for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
        dispose(item.take);
        dispose(item.take);
        break;
    }
}
function movedSource(a: string, b: string, c: string, d: string): void {
    for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}]) {
        break;
    }
    dispose(a);
}
function mixed(a: string, c: string): void {
    for (let item of [Mixed{take: a, count: 1i32}, Mixed{take: c, count: 2i32}]) {
        dispose(item.take);
        break;
    }
}
function custom(a: string, b: string, c: string, d: string): void {
    for (let item of [Custom{take: a, keep: b}, Custom{take: c, keep: d}]) {
        break;
    }
}
function textLiteral(a: string, b: string): void {
    for (let item of [a, b]) {
        dispose(item);
        break;
    }
}
"#;

const fn stats(work: usize, entries: usize, max_depth: usize) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

#[test]
fn record_yield_cleans_the_partial_item_then_the_suffix() {
    probe_with_typed(
        "iteration::record::record_yield_cleans_the_partial_item_then_the_suffix",
        SOURCE,
        |request, module, receipt, typed| {
            let capture = loader::load_installed(request.clone()).unwrap();
            let receipt_stats = stats(502_164, 64_359, 12);
            assert_eq!(receipt.statistics(), receipt_stats);
            let (exact, short) = limits(receipt_stats);
            let at = analyze_typed(&capture, exact);
            assert_eq!(at.receipt().unwrap().statistics(), receipt_stats);
            for cap in short {
                let limited = analyze_typed(&capture, cap);
                crate::limit::assert_typed_limit(&limited);
            }

            assert_pre_equality_graph_cap(receipt, (234_907, 13_094, 9));
            let graph_stats = stats(234_883, 13_167, 9);
            let (exact, short) = limits(graph_stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.statistics(), graph_stats);
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            assert!(graph.inventory_complete());
            assert_eq!(graph.test_mode(), request.test_mode);
            assert_eq!(graph.context(), receipt.context());
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&limited);
            }

            let lane = Lane::new(module, typed);
            let loop_head: &[u8] =
                b"for (let item of [Entry{take: a, keep: b}, Entry{take: c, keep: d}])";
            for (name, expected, cap) in [
                ("onBreak", Completion::Checked, stats(5_616, 1_356, 7)),
                ("grouped", Completion::Checked, stats(5_622, 1_356, 8)),
                ("scoped", Completion::Checked, stats(5_618, 1_356, 8)),
                ("wrapped", Completion::Checked, stats(6_077, 1_604, 9)),
                ("whole", Completion::Checked, stats(5_329, 1_209, 5)),
                ("exhausted", Completion::Checked, stats(5_635, 1_362, 7)),
                ("reused", Completion::SourceRejected, stats(5_666, 1_409, 7)),
                (
                    "movedSource",
                    Completion::SourceRejected,
                    stats(5_362, 1_224, 5),
                ),
                ("dead", Completion::Checked, stats(2_704, 426, 2)),
                ("pruned", Completion::Checked, stats(2_752, 456, 4)),
                ("onPostfix", Completion::Incomplete, stats(3_664, 562, 4)),
                ("textLiteral", Completion::Incomplete, stats(2_524, 350, 2)),
                ("mixed", Completion::Incomplete, stats(2_524, 350, 2)),
                ("custom", Completion::Incomplete, stats(2_536, 358, 2)),
            ] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let run = |cap| lower_body(receipt, module, &function.id, cap).unwrap();
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                let (exact, short) = limits(cap);
                let body = run(exact);
                assert_eq!(body.statistics(), cap, "{name}");
                assert_eq!(body.completion(), expected, "{name}");
                assert_eq!(root.ordinary_completion(), expected, "{name}");
                for cap in short {
                    let limited = run(cap);
                    crate::limit::assert_body_limit(&limited);
                }
                match expected {
                    Completion::Checked => {
                        for mir in [body.mir().unwrap(), root.ordinary_mir().unwrap()] {
                            if matches!(name, "dead" | "pruned") {
                                assert!(!mir.blocks.iter().any(|b| matches!(
                                    b.operation,
                                    Operation::CommitListRecordText { .. }
                                        | Operation::OpenOwnedListRecordText { .. }
                                )));
                                continue;
                            }
                            lane.check(mir, root.dependencies());
                            check_sequences(name, mir);
                        }
                    }
                    Completion::SourceRejected => {
                        assert!(body.mir().is_none() && root.ordinary_mir().is_none());
                        let [finding] = body.findings() else {
                            panic!("{name}: one reached reuse")
                        };
                        let (site, related): (&[u8], &[u8]) = if name == "reused" {
                            (b"dispose(item.take)", b"dispose(item.take)")
                        } else {
                            (b"dispose(a)", b"a")
                        };
                        assert_eq!(
                            (
                                finding.code(),
                                finding.site.span.bytes(),
                                finding.related.span.bytes()
                            ),
                            ("R0200", site, related),
                            "{name}"
                        );
                        assert!(finding.related.span.start() < finding.site.span.start());
                        assert!(graph.findings().iter().any(|f| f.code() == "R0200"
                            && f.site.node == finding.site.node
                            && f.related.node == finding.related.node));
                    }
                    _ => {
                        assert!(body.mir().is_none() && body.findings().is_empty());
                        let blocked = body.blocked_at().unwrap().span.bytes();
                        let expected: &[u8] = match name {
                            "textLiteral" => b"for (let item of [a, b])",
                            "mixed" => b"for (let item of [Mixed{take: a, count: 1i32}, Mixed{take: c, count: 2i32}])",
                            "custom" => b"for (let item of [Custom{take: a, keep: b}, Custom{take: c, keep: d}])",
                            _ => loop_head,
                        };
                        assert!(
                            blocked.starts_with(expected),
                            "{name}: {}",
                            blocked.escape_ascii()
                        );
                    }
                }
            }
        },
    );
}

/// The selected installed operations and the protected string winner.
struct Lane<'g> {
    record: &'g compiler::loader::DeclarationId,
    element: RecordTextDisposal<'g>,
    iterate: CollectionProvider<'g>,
    next: CollectionProvider<'g>,
    release: CollectionProvider<'g>,
    list: &'g compiler::loader::DeclarationId,
}

impl<'g> Lane<'g> {
    fn new(module: &'g loader::Module, typed: &TypedReport<'g>) -> Self {
        use CollectionOperation::{DisposeIterator, Iterate, Next};
        use ListTraversal::Owned;
        let provider = |operation| {
            let scope = typed
                .collection_protection()
                .find(|s| s.operation == operation)
                .unwrap();
            assert_eq!(scope.coverage, OwnershipCoverage::Clear);
            scope.provider.unwrap()
        };
        let entry = typed
            .inventory()
            .type_facts()
            .declarations()
            .find(|facts| {
                facts.declaration.id.module == module.key
                    && facts.declaration.name.as_deref() == Some("Entry")
            })
            .unwrap();
        // The callback releases these fields in reverse: `keep`, then `take`.
        let DeclarationShape::Record { fields } = &entry.shape else {
            panic!("Entry is a record")
        };
        let written: Vec<_> = fields
            .iter()
            .map(|field| {
                assert!(
                    matches!(field.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "string")
                );
                field.declaration.name.as_deref().unwrap()
            })
            .collect();
        assert_eq!(written, ["take", "keep"]);
        let protected = typed
            .primitive_protection()
            .iter()
            .find(|p| p.primitive == Primitive::String && p.role == PrimitiveRole::Dispose)
            .unwrap();
        assert_eq!(protected.coverage, PrimitiveCoverage::Clear);
        let winner = protected.required_winner.unwrap();
        let binding = typed
            .primitive_providers()
            .iter()
            .find(|p| &p.implementation.id == winner)
            .unwrap()
            .binding
            .unwrap();
        assert_eq!(binding.operation, PrimitiveOperation::DisposeText);
        let record = &entry.declaration.id;
        Self {
            record,
            element: RecordTextDisposal {
                record,
                field: TextElementDisposal {
                    implementation: winner,
                    bridge: &binding.declaration.id,
                },
            },
            iterate: provider(Iterate(Owned)),
            next: provider(Next(Owned)),
            release: provider(DisposeIterator(Owned)),
            list: &typed.list_role().unwrap().declaration.id,
        }
    }

    fn check(&self, mir: &Mir<'g>, dependencies: &[Dependency<'g>]) {
        use CollectionOperation::{DisposeIterator, Iterate, Next};
        use ListTraversal::Owned;
        assert!(mir.cancellation().is_none() && mir.replacement_cancellation().is_none());
        let [commit] = blocks(mir, |op| {
            matches!(op, Operation::CommitListRecordText { .. })
        })[..] else {
            panic!("commit the staged records once")
        };
        let [open] = blocks(mir, |op| {
            matches!(op, Operation::OpenOwnedListRecordText { .. })
        })[..] else {
            panic!("open the committed List once")
        };
        let Operation::CommitListRecordText {
            into: list,
            backing,
            provider: artifact,
            ..
        } = &commit.operation
        else {
            unreachable!()
        };
        let Operation::OpenOwnedListRecordText {
            source,
            cursor,
            source_backing,
            cursor_backing,
            provider,
        } = &open.operation
        else {
            unreachable!()
        };
        assert!(same_place(*list, *source));
        assert_eq!(
            (
                backing.storage,
                source_backing.storage,
                cursor_backing.storage
            ),
            (
                CollectionStorage::Elements,
                CollectionStorage::Elements,
                CollectionStorage::UnyieldedElements
            )
        );
        assert!(std::ptr::eq(&backing.declaration.id, self.list));
        assert!(std::ptr::eq(&source_backing.declaration.id, self.list));
        assert_eq!(*artifact, provider.artifact);
        assert_eq!(&provider.implementation.id, &self.iterate.implementation.id);
        for block in &mir.blocks {
            match &block.operation {
                Operation::AdvanceOwnedListRecordText {
                    cursor: advanced,
                    provider,
                    ..
                } => {
                    assert!(same_place(*advanced, *cursor));
                    assert_eq!(&provider.implementation.id, &self.next.implementation.id);
                }
                Operation::Cleanup(actions) => {
                    for action in actions {
                        if let Obligation::DisposeOwnedListIterRecordText {
                            cursor: retired,
                            implementation,
                            bridge,
                            element,
                        } = action
                        {
                            assert!(same_place(*retired, *cursor));
                            assert_eq!(
                                (*implementation, *bridge, *element),
                                (
                                    &self.release.implementation.id,
                                    &self.release.bridge.id,
                                    self.element
                                )
                            );
                        }
                    }
                }
                Operation::DisposeText { element, .. } => {
                    assert_eq!(*element, self.element.field);
                }
                _ => {}
            }
        }
        for (operation, p) in [
            (Iterate(Owned), self.iterate),
            (Next(Owned), self.next),
            (DisposeIterator(Owned), self.release),
        ] {
            assert!(
                dependencies
                    .iter()
                    .any(|d| d.declaration == &p.implementation.id
                        && d.contract
                            == DependencyContract::ListRecordTextCollection {
                                operation,
                                bridge: &p.bridge.id
                            })
            );
        }
        assert!(
            dependencies
                .iter()
                .any(|d| d.declaration == self.element.field.implementation
                    && d.contract
                        == DependencyContract::RecordTextElementDisposal {
                            record: self.record,
                            bridge: self.element.field.bridge
                        })
        );
    }
}

fn blocks<'a, 'g>(mir: &'a Mir<'g>, keep: impl Fn(&Operation<'g>) -> bool) -> Vec<&'a Block<'g>> {
    mir.blocks.iter().filter(|b| keep(&b.operation)).collect()
}

/// Walk from entry, taking `choices` at each branch in order, until a return
/// or a revisited block. Every operation is one entry of a single list.
fn walk(mir: &Mir<'_>, choices: &[bool]) -> Vec<String> {
    let name = |p: &Place<'_>| {
        let text = p.root.span.bytes();
        let mut name = if text.starts_with(b"for (") {
            "cursor".to_owned()
        } else if text.starts_with(b"[") {
            "list".to_owned()
        } else {
            text.escape_ascii().to_string()
        };
        if let Some(field) = p.field() {
            name.push('.');
            name.push_str(field.name.as_deref().unwrap());
        }
        name
    };
    let mut events = Vec::new();
    let mut seen = vec![false; mir.blocks.len()];
    let mut choices = choices.iter();
    let mut pc = mir.entry;
    loop {
        if std::mem::replace(&mut seen[pc], true) {
            events.push(format!("back to {pc}"));
            return events;
        }
        let block = &mir.blocks[pc];
        match &block.operation {
            Operation::None => {}
            Operation::Move { from, into } => {
                events.push(format!("move {} -> {}", name(from), name(&into.unwrap())))
            }
            Operation::Copy { from, into: None } => events.push(format!("read {}", name(from))),
            Operation::Initialize(p) => events.push(format!("init {}", name(p))),
            Operation::CommitListRecordText { elements, into, .. } => events.push(format!(
                "commit {} -> {}",
                elements.iter().map(name).collect::<Vec<_>>().join(", "),
                name(into)
            )),
            Operation::OpenOwnedListRecordText { source, cursor, .. } => {
                events.push(format!("open {} -> {}", name(source), name(cursor)))
            }
            Operation::AdvanceOwnedListRecordText { cursor, into, .. } => {
                events.push(format!("advance {} -> {}", name(cursor), name(into)))
            }
            Operation::DisposeText { operand, .. } => {
                events.push(format!("dispose text {}", name(operand)))
            }
            Operation::Drop(p) => events.push(format!("drop {}", name(p))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    events.push(match action {
                        Obligation::Drop(p) => format!("drop {}", name(p)),
                        Obligation::EndStorage(p) => format!("end {}", name(p)),
                        Obligation::DisposeOwnedListIterRecordText { cursor, .. } => {
                            format!("suffix {}", name(cursor))
                        }
                        other => panic!("unexpected cleanup {}", variant(other)),
                    })
                }
            }
            other => panic!("unexpected operation {}", variant(other)),
        }
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Branch { yes, no } => {
                let yes_taken = *choices.next().expect("a choice per branch");
                events.push(format!("branch {}", if yes_taken { "yes" } else { "no" }));
                pc = if yes_taken { yes } else { no };
            }
            Terminator::Return => {
                events.push("return".to_owned());
                return events;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

fn variant(value: &impl std::fmt::Debug) -> String {
    format!("{value:?}")
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .collect()
}

fn check_sequences(name: &str, mir: &Mir<'_>) {
    let stage = |take: &str, keep: &str, record: &str| {
        [
            format!("move {take}: string -> {record}.take"),
            format!("move {keep}: string -> {record}.keep"),
            format!("init {record}"),
        ]
    };
    let mut prefix: Vec<String> = Vec::new();
    if name == "wrapped" {
        prefix.extend(["read flag: bool".to_owned(), "branch yes".to_owned()]);
    }
    let (first, second) = if name == "grouped" {
        ("Entry{take: a, keep: b}", "(Entry{take: c, keep: d})")
    } else {
        ("Entry{take: a, keep: b}", "Entry{take: c, keep: d}")
    };
    prefix.extend(stage("a", "b", first));
    prefix.extend(stage("c", "d", second));
    let item = if name == "grouped" { "(item)" } else { "item" };
    prefix.extend([
        format!("commit {first}, {second} -> list"),
        "open list -> cursor".to_owned(),
        format!("advance cursor -> {item}"),
    ]);
    let expect = |tail: &[&str]| -> Vec<String> {
        prefix
            .iter()
            .cloned()
            .chain(tail.iter().map(|s| (*s).to_owned()))
            .collect()
    };
    let control: &[bool] = if name == "wrapped" { &[true] } else { &[] };
    let path = |some: bool| walk(mir, &[control, &[some]].concat());
    // Exhaustion: the empty cursor is the only owner left.
    let exhausted_tail: &[&str] = if name == "wrapped" {
        &["branch no", "suffix cursor", "end flag: bool", "return"]
    } else {
        &["branch no", "suffix cursor", "return"]
    };
    assert_eq!(path(false), expect(exhausted_tail), "{name}: None");
    let some: Vec<&str> = match name {
        // Only `keep` remains of the current item; the suffix holds whole records.
        "onBreak" | "scoped" => vec![
            "branch yes",
            "dispose text item.take",
            "init dispose(item.take)",
            "drop item.keep",
            "end item",
            "suffix cursor",
            "return",
        ],
        "grouped" => vec![
            "branch yes",
            "dispose text (item).take",
            "init dispose((item.take))",
            "drop (item).keep",
            "end (item)",
            "suffix cursor",
            "return",
        ],
        "wrapped" => vec![
            "branch yes",
            "dispose text item.take",
            "init dispose(item.take)",
            "drop item.keep",
            "end item",
            "suffix cursor",
            "end flag: bool",
            "return",
        ],
        // The whole current item drops in reverse field order first.
        "whole" => vec![
            "branch yes",
            "drop item.keep",
            "drop item.take",
            "end item",
            "suffix cursor",
            "return",
        ],
        // The current item ends before the next advance; nothing repeats.
        "exhausted" => vec![
            "branch yes",
            "dispose text item.keep",
            "init dispose(item.keep)",
            "drop item.take",
            "end item",
        ],
        _ => unreachable!(),
    };
    let mut observed = path(true);
    if name == "exhausted" {
        let back = observed.pop().unwrap();
        assert!(back.starts_with("back to "), "{back}");
        let head = mir
            .blocks
            .iter()
            .position(|b| matches!(b.operation, Operation::AdvanceOwnedListRecordText { .. }))
            .unwrap();
        assert_eq!(back, format!("back to {head}"));
    }
    assert_eq!(observed, expect(&some), "{name}: Some");
}

#[test]
fn string_on_break_keeps_its_cap() {
    // Measured at 057c5005, before the record lane. Row C5 of the gap
    // analysis cites 2,547/312/6, the ledger's figure at 7c145d3b.
    probe(
        "iteration::record::string_on_break_keeps_its_cap",
        super::text::SOURCE,
        |_, module, receipt| {
            let function = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("onBreak"))
                .unwrap();
            let cap = stats(2_851, 457, 6);
            let (exact, short) = limits(cap);
            let body = lower_body(receipt, module, &function.id, exact).unwrap();
            assert_eq!(body.statistics(), cap);
            assert_eq!(body.completion(), Completion::Checked);
            for cap in short {
                let limited = lower_body(receipt, module, &function.id, cap).unwrap();
                crate::limit::assert_body_limit(&limited);
            }
        },
    );
}

#[test]
fn linked_record_suffix_releases_whole_remaining_records_in_reverse() {
    let record = |take: &str, keep: &str| (take.to_owned(), keep.to_owned());
    for (visits, expected) in [
        (1, &["f", "e", "d", "c"][..]),
        (2, &["f", "e"][..]),
        (3, &[][..]),
    ] {
        let mut cursor =
            List::from_elements([record("a", "b"), record("c", "d"), record("e", "f")])
                .into_owned_iter();
        for _ in 0..visits {
            // The current item is the binder's: `take` is disposed, `keep` dropped.
            let (take, keep) = cursor.next().unwrap();
            PrimitiveOperation::DisposeText.dispose_text(take).unwrap();
            PrimitiveOperation::DisposeText.dispose_text(keep).unwrap();
        }
        let mut released = Vec::new();
        // RecordTextDisposal: each field in reverse declaration order.
        cursor.dispose_with(|(take, keep)| {
            for field in [keep, take] {
                released.push(field.clone());
                PrimitiveOperation::DisposeText.dispose_text(field).unwrap();
            }
        });
        assert_eq!(released, expected);
    }
}
