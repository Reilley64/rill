use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{GraphCompletion, GraphReport, analyze_ownership, lower_root};
use compiler::analysis::scalars::BodyOwner;
use compiler::analysis::typed::TypedCompletion;
use compiler::loader::DeclarationKind;

const O15: &str = r#"
function counter(): i32 {
  let mut initial = 0i32;
  let mut next: mut () => i32 = () => {
    initial += 1i32;
    return initial;
  };
  let first = next();
  let second = next();
  return first * 10i32 + second;
}
expect counter() == 12i32;
"#;

const ORIGINAL_COUNTER: &str = r#"
function originalCounter(): i32 {
    let mut initial = 0i32;
    let mut next: mut () => i32 = () => { initial += 1i32; return initial; };
    let first = next();
    let second = next();
    return first * 10i32 + second;
}
expect originalCounter() == 12i32;
"#;

#[test]
fn rfc_counter_has_targeted_and_graph_mir() {
    counter_probe(
        "o15_arithmetic::rfc_counter_has_targeted_and_graph_mir",
        O15,
        "counter",
    );
}

#[test]
fn ledger_counter_has_targeted_and_graph_mir() {
    counter_probe(
        "o15_arithmetic::ledger_counter_has_targeted_and_graph_mir",
        ORIGINAL_COUNTER,
        "originalCounter",
    );
}

fn counter_probe(test: &str, source: &str, name: &str) {
    probe(test, source, |request, module, receipt| {
        let decl = named(module, name);
        let query = |limits| lower_body(receipt, module, &decl.id, limits).unwrap();
        let target = query(AnalysisLimits::default());
        assert_eq!(
            target.completion(),
            Completion::Checked,
            "{:?}",
            target
                .blocked_at()
                .map(|s| s.span.bytes().escape_ascii().to_string())
        );
        assert!(target.findings().is_empty());
        let (exact, short) = caps(target.statistics());
        let replay = query(exact);
        assert_eq!(replay.statistics(), target.statistics());
        check_counter(replay.mir().unwrap());
        for short in short {
            let limited = query(short);
            assert_eq!(limited.completion(), Completion::ResourceLimit);
            assert!(limited.mir().is_none() && limited.findings().is_empty());
        }

        let graph = analyze_ownership(receipt, AnalysisLimits::default());
        let (exact, short) = caps(graph.statistics());
        let replay = analyze_ownership(receipt, exact);
        assert_eq!(replay.statistics(), graph.statistics());
        assert_eq!(replay.completion(), graph.completion());
        for graph in [&graph, &replay] {
            let root = graph_root(graph, module, decl.node);
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            check_counter(root.ordinary_mir().unwrap());
        }
        for short in short {
            assert_eq!(
                analyze_ownership(receipt, short).completion(),
                GraphCompletion::ResourceLimit
            );
        }

        // Equality/call evaluation and the expectation outcome are separate
        // from the function's ordinary arithmetic proof.
        let expect = module
            .declarations
            .iter()
            .find(|d| d.id.kind == DeclarationKind::Expectation)
            .unwrap();
        let expectation = lower_root(
            receipt,
            module,
            BodyOwner::Declaration(&expect.id),
            AnalysisLimits::default(),
        )
        .unwrap();
        let root = graph_root(&graph, module, expect.node);
        let equality = format!("{name}() == 12i32");
        for (completion, mir, blocked) in [
            (
                expectation.completion(),
                expectation.mir(),
                expectation.blocked_at(),
            ),
            (
                root.ordinary_completion(),
                root.ordinary_mir(),
                root.blocked_at(),
            ),
        ] {
            assert_eq!(completion, Completion::Incomplete);
            assert!(mir.is_none());
            assert_eq!(blocked.unwrap().span.bytes(), equality.as_bytes());
        }

        let foreign = loader::load_installed(request.clone()).unwrap();
        let foreign = foreign
            .modules()
            .iter()
            .find(|m| m.key == module.key)
            .unwrap();
        let zero = AnalysisLimits {
            max_work: 0,
            max_entries: 0,
            max_depth: 0,
        };
        assert!(lower_body(receipt, foreign, &decl.id, zero).is_err());
        assert!(lower_body(receipt, module, &named(foreign, name).id, zero).is_err());
    });
}

fn check_counter(mir: &Mir<'_>) {
    let path = linear(mir, mir.entry);
    let effects: Vec<_> = path
        .iter()
        .copied()
        .filter(|op| matches!(op, Operation::InvokeMutableI32Cell { .. }) || arithmetic_effect(op))
        .collect();
    let [
        Operation::InvokeMutableI32Cell {
            receiver,
            origin,
            field,
            into: first,
            addend: 1,
        },
        Operation::InvokeMutableI32Cell {
            receiver: second_receiver,
            origin: second_origin,
            field: second_field,
            into: second,
            addend: 1,
        },
        mul,
        add,
        export,
    ] = effects.as_slice()
    else {
        panic!("expected both same-cell calls, checked arithmetic, then export");
    };
    assert_eq!(receiver.root.node, second_receiver.root.node);
    assert_eq!(origin.node, second_origin.node);
    assert_eq!(field.node, second_field.node);
    assert_ne!(first.root.node, second.root.node);
    let inputs = check_mix(&[*mul, *add, *export], 10, [b"first", b"second"]);
    assert_eq!(inputs, [first.root.node, second.root.node]);
    let exported = path
        .iter()
        .position(|op| matches!(op, Operation::Export(_)))
        .unwrap();
    let ended: Vec<_> = path
        .iter()
        .enumerate()
        .filter_map(|(i, op)| {
            matches!(op, Operation::Cleanup(actions) if actions.iter().any(|a|
            matches!(a, Obligation::EndStorage(place) if place.root.node == receiver.root.node)))
            .then_some(i)
        })
        .collect();
    assert!(matches!(ended.as_slice(), [i] if *i > exported));
}

fn arithmetic_effect(op: &Operation<'_>) -> bool {
    matches!(
        op,
        Operation::CheckedI32Multiply { .. }
            | Operation::CheckedI32Add { .. }
            | Operation::Export(_)
    )
}

fn check_mix(ops: &[&Operation<'_>], factor: i32, inputs: [&[u8]; 2]) -> [usize; 2] {
    let [
        Operation::CheckedI32Multiply {
            left,
            literal,
            into: product,
        },
        Operation::CheckedI32Add {
            left: add_left,
            right,
            into: sum,
        },
        Operation::Export(output),
    ] = ops
    else {
        panic!("expected checked multiplication, addition, then export");
    };
    assert_eq!(*literal, factor);
    assert_eq!([left.root.span.bytes(), right.root.span.bytes()], inputs);
    assert_eq!(add_left.root.node, product.root.node);
    assert_eq!(output.root.node, sum.root.node);
    [left.root.node, right.root.node]
}

#[test]
fn checked_arithmetic_is_not_a_counter_special_case() {
    probe(
        "o15_arithmetic::checked_arithmetic_is_not_a_counter_special_case",
        r#"
function mix(left: i32, right: i32): i32 { return (left * 0x11i32) + right; }
function otherWidth(left: i64, right: i64): i64 { return left * 10i64 + right; }
function computedMultiplier(left: i32, right: i32): i32 { return left * (4i32 + 6i32) + right; }
function computedRight(left: i32, right: i32): i32 { return left + (right * 10i32); }
"#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let query = |name| {
                lower_body(
                    receipt,
                    module,
                    &named(module, name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let mix = query("mix");
            let root = graph_root(&graph, module, named(module, "mix").node);
            for mir in [mix.mir().unwrap(), root.ordinary_mir().unwrap()] {
                let ops: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| arithmetic_effect(op))
                    .collect();
                check_mix(&ops, 17, [b"left: i32", b"right: i32"]);
            }
            for (name, expected) in [
                ("otherWidth", "left * 10i64 + right"),
                ("computedMultiplier", "left * (4i32 + 6i32)"),
                ("computedRight", "left + (right * 10i32)"),
            ] {
                let target = query(name);
                let root = graph_root(&graph, module, named(module, name).node);
                assert!(target.findings().is_empty());
                for (completion, mir, blocked) in [
                    (target.completion(), target.mir(), target.blocked_at()),
                    (
                        root.ordinary_completion(),
                        root.ordinary_mir(),
                        root.blocked_at(),
                    ),
                ] {
                    assert_eq!(completion, Completion::Incomplete, "{name}");
                    assert!(mir.is_none());
                    assert_eq!(blocked.unwrap().span.bytes(), expected.as_bytes());
                }
            }
        },
    );
}

#[test]
fn mixed_widths_do_not_issue_a_typed_receipt() {
    probe_report(
        "o15_arithmetic::mixed_widths_do_not_issue_a_typed_receipt",
        "function mixed(left: i32, right: i64): i32 { return left * 10i32 + right; }",
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            assert!(
                typed
                    .bodies()
                    .findings()
                    .iter()
                    .any(|finding| finding.kind.code() == Some("R0100"))
            );
        },
    );
}

#[test]
fn checked_traps_are_retained_when_evaluated_but_not_hoisted() {
    probe(
        "o15_arithmetic::checked_traps_are_retained_when_evaluated_but_not_hoisted",
        r#"
function trapAdd(owner: string): i32 {
    let high = 2147483647i32;
    let one = 1i32;
    return high + one;
}
function discard(owner: string): i32 {
    let high = 2147483647i32;
    let one = 1i32;
    high * 2i32 + one;
    return 0i32;
}
function skip(owner: string): i32 {
    let high = 2147483647i32;
    let one = 1i32;
    if (false) { high * 2i32 + one; }
    return 0i32;
}
"#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, expected) in [
                ("trapAdd", &[b"+".as_slice()][..]),
                ("discard", &[b"*".as_slice(), b"+".as_slice()][..]),
                ("skip", &[][..]),
            ] {
                let decl = named(module, name);
                let target =
                    lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
                let root = graph_root(&graph, module, decl.node);
                for mir in [target.mir().unwrap(), root.ordinary_mir().unwrap()] {
                    let path = linear(mir, mir.entry);
                    let mut checked = Vec::new();
                    let mut export = None;
                    let mut dropped = 0;
                    for op in path {
                        match op {
                            Operation::CheckedI32Multiply {
                                left, literal: 2, ..
                            } => {
                                assert_eq!(left.root.span.bytes(), b"high");
                                assert!(export.is_none());
                                checked.push(b"*".as_slice());
                            }
                            Operation::CheckedI32Add { right, into, .. } => {
                                assert_eq!(right.root.span.bytes(), b"one");
                                assert!(export.is_none());
                                if name == "trapAdd" {
                                    assert_eq!(into.root.span.bytes(), b"high + one");
                                }
                                checked.push(b"+".as_slice());
                            }
                            Operation::Export(output) => export = Some(output.root.span.bytes()),
                            Operation::Drop(place)
                                if place.root.span.bytes() == b"owner: string" =>
                            {
                                assert!(export.is_some());
                                dropped += 1;
                            }
                            Operation::Cleanup(actions) => {
                                for action in actions {
                                    if matches!(action, Obligation::Drop(place) if place.root.span.bytes() == b"owner: string")
                                    {
                                        assert!(export.is_some());
                                        dropped += 1;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    assert_eq!(checked, expected, "{name}");
                    assert_eq!(dropped, 1, "{name}");
                    assert_eq!(
                        export,
                        Some(if name == "trapAdd" {
                            b"high + one".as_slice()
                        } else {
                            b"0i32".as_slice()
                        })
                    );
                }
            }
        },
    );
}

fn caps(stats: Statistics) -> (AnalysisLimits, [AnalysisLimits; 3]) {
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    (
        exact,
        [
            AnalysisLimits {
                max_work: stats.work - 1,
                ..exact
            },
            AnalysisLimits {
                max_entries: stats.entries - 1,
                ..exact
            },
            AnalysisLimits {
                max_depth: stats.max_depth - 1,
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
    node: usize,
) -> &'a compiler::analysis::ownership::RootReport<'g> {
    graph
        .roots()
        .iter()
        .find(|r| r.site().module == &module.key && r.site().node == node)
        .unwrap()
}
