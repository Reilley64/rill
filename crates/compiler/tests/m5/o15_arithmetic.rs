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

        // The expectation root calls the function and compares its i32 result
        // (ri-785.9). This is MIR for the root, not the observed value 12.
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
        assert!(expectation.findings().is_empty());
        for (completion, mir) in [
            (expectation.completion(), expectation.mir()),
            (root.ordinary_completion(), root.ordinary_mir()),
        ] {
            assert_eq!(completion, Completion::Checked);
            check_expectation(mir.unwrap(), &decl.id, &format!("{name}()"), "12i32");
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

#[test]
fn rfc_counter_caps_include_the_charged_local_reads() {
    probe(
        "o15_arithmetic::rfc_counter_caps_include_the_charged_local_reads",
        O15,
        |_, module, receipt| {
            let decl = named(module, "counter");
            let query = |cap| lower_body(receipt, module, &decl.id, cap).unwrap();
            // The reads of `first` and `second` charge "main.rill" (10) three
            // times each in place of one step: 2 x (10 x 3 - 1) = 58 work.
            let stats = statistics(2562, 329, 6);
            let (exact, short) = caps(stats);
            let target = query(exact);
            assert_eq!(target.completion(), Completion::Checked);
            assert_eq!(target.statistics(), stats);
            for cap in short {
                let limited = query(cap);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none() && limited.findings().is_empty());
            }
            // Metering rule 11 (ri-785.9): the A2 graph cap 187,543/4,297/6
            // gives ResourceLimit now that the i32 equality source root
            // (+316/+134, depth 5) and the counter expectation root
            // (+184/+65) are checked.
            assert_eq!(
                analyze_ownership(receipt, caps(statistics(187543, 4297, 6)).0).completion(),
                GraphCompletion::ResourceLimit
            );
            let graph_stats = statistics(188043, 4496, 6);
            let (exact, short) = caps(graph_stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert_eq!(graph.statistics(), graph_stats);
            let root = graph_root(&graph, module, decl.node);
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            for cap in short {
                assert_eq!(
                    analyze_ownership(receipt, cap).completion(),
                    GraphCompletion::ResourceLimit
                );
            }
        },
    );
}

#[test]
fn counter_expectation_checks_past_its_incomplete_cap() {
    // The parked fixture of task 5dd0ff7d: the 17-source typed baseline plus
    // the counter. Before the i32 equality expression (ri-785.9) the
    // expectation root was Incomplete at the comparison at 2,279/210/1; that
    // cap now gives ResourceLimit.
    let baseline = include_str!("../../../../tests/installed/typed/main.rill");
    let checks = include_str!("../../../../tests/installed/typed/checks.rill");
    let counter = ORIGINAL_COUNTER.replace("originalCounter", "counter");
    installed(
        "o15_arithmetic::counter_expectation_checks_past_its_incomplete_cap",
        |request| {
            fs::write(
                &request.entry,
                format!("{baseline}{}", counter.trim_start()),
            )
            .unwrap();
            fs::write(request.entry.with_file_name("checks.rill"), checks).unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let module = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                    .unwrap();
                let expect = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Expectation
                            && d.span.bytes().starts_with(b"expect counter(")
                    })
                    .unwrap();
                let query = |cap| {
                    lower_root(&receipt, module, BodyOwner::Declaration(&expect.id), cap).unwrap()
                };
                let incomplete = query(caps(statistics(2279, 210, 1)).0);
                assert_eq!(incomplete.completion(), Completion::ResourceLimit);
                assert!(incomplete.mir().is_none() && incomplete.findings().is_empty());

                let stats = statistics(5329, 781, 6);
                assert_eq!(query(AnalysisLimits::default()).statistics(), stats);
                let (exact, short) = caps(stats);
                let target = query(exact);
                assert_eq!(target.completion(), Completion::Checked);
                assert_eq!(target.statistics(), stats);
                assert!(target.findings().is_empty());
                let counter = &named(module, "counter").id;
                check_expectation(target.mir().unwrap(), counter, "counter()", "12i32");
                let graph = analyze_ownership(&receipt, AnalysisLimits::default());
                let root = graph_root(&graph, module, expect.node);
                assert_eq!(root.ordinary_completion(), Completion::Checked);
                check_expectation(root.ordinary_mir().unwrap(), counter, "counter()", "12i32");
                for cap in short {
                    let limited = query(cap);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

/// The expectation evaluates the call, then the literal, compares the two
/// scratch values and exports the comparison.
fn check_expectation(mir: &Mir<'_>, callee: &loader::DeclarationId, call: &str, literal: &str) {
    let path = straight_blocks(mir, mir.entry);
    assert_eq!(
        path.last().unwrap().terminator,
        Terminator::FinishExpectation
    );
    let ops = path
        .iter()
        .map(|block| &block.operation)
        .filter(|op| !matches!(op, Operation::None))
        .collect::<Vec<_>>();
    let [
        Operation::Call(called),
        Operation::Initialize(right_value),
        Operation::EqualI32 { left, right, into },
        Operation::Export(exported),
    ] = ops.as_slice()
    else {
        panic!("expected call, literal, comparison and export: {ops:?}");
    };
    assert_eq!(*called, callee);
    assert_eq!(left.root.span.bytes(), call.as_bytes());
    assert_eq!(right.root.span.bytes(), literal.as_bytes());
    assert_eq!(right.root.node, right_value.root.node);
    assert_eq!(
        into.root.span.bytes(),
        format!("{call} == {literal}").as_bytes()
    );
    assert_eq!(exported.root.node, into.root.node);
}

/// Exact caps measured at 1c27e7c9, before the i32 equality expression
/// (ri-785.9). Nothing here is admitted, so each root keeps its cap and
/// blocker, and the whole graph grows by the i32 equality source root alone.
#[test]
fn unsupported_equality_keeps_pre_equality_caps_and_sites() {
    probe(
        "o15_arithmetic::unsupported_equality_keeps_pre_equality_caps_and_sites",
        r#"
function seven(): i32 { return 7i32; }
function ready(): bool { return true; }
function wide(): i64 { return 7i64; }
function different(left: i32, right: i32): bool { return left != right; }
function computed(left: i32, right: i32): bool { return left * right == right; }
expect seven() != 7i32;
expect ready() == true;
expect wide() == 7i64;
expect seven() < 8i32;
expect seven() + 1i32 == 8i32;
expect ((seven() + 1i32)) == 8i32;
expect seven() == 7i32 && true;
"#,
        |_, module, receipt| {
            // Whole-graph cap before: the i32 equality source root alone adds
            // 316 work and 134 entries and sets depth 5 (metering rule 11).
            let before = statistics(190397, 4219, 3);
            assert_eq!(
                analyze_ownership(receipt, caps(before).0).completion(),
                GraphCompletion::ResourceLimit
            );
            let graph_stats = statistics(190713, 4353, 5);
            let (exact, short) = caps(graph_stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert_eq!(graph.statistics(), graph_stats);
            for cap in short {
                assert_eq!(
                    analyze_ownership(receipt, cap).completion(),
                    GraphCompletion::ResourceLimit
                );
            }
            for (root, work, entries, depth, blocker) in [
                ("function different(", 1975, 80, 3, "left != right"),
                ("function computed(", 1975, 80, 3, "left * right == right"),
                ("expect seven() != 7i32;", 1971, 68, 1, "seven() != 7i32"),
                ("expect ready() == true;", 1972, 68, 1, "ready() == true"),
                ("expect wide() == 7i64;", 1973, 68, 1, "wide() == 7i64"),
                ("expect seven() < 8i32;", 1974, 68, 1, "seven() < 8i32"),
                (
                    "expect seven() + 1i32 == 8i32;",
                    1975,
                    68,
                    1,
                    "seven() + 1i32 == 8i32",
                ),
                (
                    "expect ((seven() + 1i32)) == 8i32;",
                    1976,
                    68,
                    1,
                    "((seven() + 1i32)) == 8i32",
                ),
                (
                    "expect seven() == 7i32 && true;",
                    1977,
                    68,
                    1,
                    "seven() == 7i32 && true",
                ),
            ] {
                let decl = written(module, root);
                let query = |cap| {
                    lower_root(receipt, module, BodyOwner::Declaration(&decl.id), cap).unwrap()
                };
                let stats = statistics(work, entries, depth);
                let (exact, short) = caps(stats);
                let target = query(exact);
                assert_eq!(target.completion(), Completion::Incomplete, "{root}");
                assert_eq!(target.statistics(), stats, "{root}");
                assert!(target.mir().is_none() && target.findings().is_empty());
                assert_eq!(
                    target.blocked_at().unwrap().span.bytes(),
                    blocker.as_bytes()
                );
                let graph_root = graph_root(&graph, module, decl.node);
                assert_eq!(graph_root.ordinary_completion(), Completion::Incomplete);
                assert!(graph_root.ordinary_mir().is_none());
                assert_eq!(
                    graph_root.blocked_at().unwrap().span.bytes(),
                    blocker.as_bytes()
                );
                for cap in short {
                    let limited = query(cap);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{root}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

#[test]
fn i32_equality_is_not_a_counter_special_case() {
    probe(
        "o15_arithmetic::i32_equality_is_not_a_counter_special_case",
        r#"
function seven(): i32 { return 7i32; }
function same(left: i32, right: i32): bool { return left == right; }
function scaled(left: i32, right: i32): bool { return ((left * 10i32 + right)) == (right); }
expect seven() == 7i32;
expect (((seven()))) == ((7i32));
expect 7i32 == seven();
"#,
        |_, module, receipt| {
            // Before: Incomplete at each comparison, 188,285/4,095/3 for the graph.
            let before = statistics(188285, 4095, 3);
            assert_eq!(
                analyze_ownership(receipt, caps(before).0).completion(),
                GraphCompletion::ResourceLimit
            );
            let graph_stats = statistics(189812, 4724, 8);
            let (exact, short) = caps(graph_stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert_eq!(graph.statistics(), graph_stats);
            for cap in short {
                assert_eq!(
                    analyze_ownership(receipt, cap).completion(),
                    GraphCompletion::ResourceLimit
                );
            }
            let seven = &named(module, "seven").id;
            // Groups add depth, not operations: the grouped expectation is
            // two deeper than the bare one and fails one short of it.
            for (root, work, entries, depth) in [
                ("function same(", 2172, 179, 4),
                ("function scaled(", 2339, 237, 8),
                ("expect seven() == 7i32;", 4012, 185, 3),
                ("expect (((seven()))) == ((7i32));", 4023, 185, 5),
                ("expect 7i32 == seven();", 4014, 185, 3),
            ] {
                let decl = written(module, root);
                let query = |cap| {
                    lower_root(receipt, module, BodyOwner::Declaration(&decl.id), cap).unwrap()
                };
                let stats = statistics(work, entries, depth);
                assert_eq!(
                    query(AnalysisLimits::default()).statistics(),
                    stats,
                    "{root}"
                );
                let (exact, short) = caps(stats);
                let target = query(exact);
                assert_eq!(target.completion(), Completion::Checked, "{root}");
                assert!(target.findings().is_empty());
                let graph_root = graph_root(&graph, module, decl.node);
                assert_eq!(graph_root.ordinary_completion(), Completion::Checked);
                for mir in [target.mir().unwrap(), graph_root.ordinary_mir().unwrap()] {
                    match root {
                        "function same(" => {
                            let [
                                Operation::EqualI32 { left, right, into },
                                Operation::Export(exported),
                            ] = equality_effects(mir, Terminator::Return).as_slice()
                            else {
                                panic!("{root}: expected comparison then export");
                            };
                            assert_eq!(left.root.span.bytes(), b"left");
                            assert_eq!(right.root.span.bytes(), b"right");
                            assert_eq!(into.root.span.bytes(), b"left == right");
                            assert_eq!(exported.root.node, into.root.node);
                        }
                        "function scaled(" => {
                            let [
                                Operation::CheckedI32Multiply { .. },
                                Operation::CheckedI32Add { into: sum, .. },
                                Operation::EqualI32 { left, right, into },
                                Operation::Export(exported),
                            ] = equality_effects(mir, Terminator::Return).as_slice()
                            else {
                                panic!("{root}: expected arithmetic, comparison, export");
                            };
                            assert_eq!(left.root.node, sum.root.node);
                            assert_eq!(left.root.span.bytes(), b"((left * 10i32 + right))");
                            assert_eq!(right.root.span.bytes(), b"(right)");
                            assert_eq!(exported.root.node, into.root.node);
                        }
                        "expect seven() == 7i32;" => {
                            check_expectation(mir, seven, "seven()", "7i32")
                        }
                        "expect (((seven()))) == ((7i32));" => {
                            check_expectation(mir, seven, "(((seven())))", "((7i32))")
                        }
                        _ => {
                            // The literal is evaluated first when it is written first.
                            let [
                                Operation::Initialize(literal),
                                Operation::Call(called),
                                Operation::EqualI32 { left, right, into },
                                Operation::Export(exported),
                            ] = equality_effects(mir, Terminator::FinishExpectation).as_slice()
                            else {
                                panic!("{root}: expected literal, call, comparison, export");
                            };
                            assert_eq!(*called, seven);
                            assert_eq!(left.root.node, literal.root.node);
                            assert_eq!(right.root.span.bytes(), b"seven()");
                            assert_eq!(into.root.span.bytes(), b"7i32 == seven()");
                            assert_eq!(exported.root.node, into.root.node);
                        }
                    }
                }
                for cap in short {
                    let limited = query(cap);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{root}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

/// Calls, literals, checked arithmetic, comparisons and exports on the
/// straight path, ending at `end`.
fn equality_effects<'a, 'g>(mir: &'a Mir<'g>, end: Terminator) -> Vec<&'a Operation<'g>> {
    let path = straight_blocks(mir, mir.entry);
    assert_eq!(path.last().unwrap().terminator, end);
    path.iter()
        .map(|block| &block.operation)
        .filter(|op| {
            arithmetic_effect(op)
                || matches!(
                    op,
                    Operation::Call(_) | Operation::Initialize(_) | Operation::EqualI32 { .. }
                )
        })
        .collect()
}

fn written<'g>(module: &'g loader::Module, prefix: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.span.bytes().starts_with(prefix.as_bytes()))
        .unwrap()
}

#[test]
fn arithmetic_local_reads_charge_their_module_path() {
    // Review c61a0a14: arithmetic looked up locals keyed by a module path
    // without charging it. The same body in a longer-named module must cost
    // more than a control body that reads the same locals without arithmetic.
    const SHORT: &str = "short.rill";
    const LONG: &str = "arithmetic_read_from_a_module_with_a_much_longer_path.rill";
    const BODIES: &str = r#"
export function mix(left: i32, right: i32): i32 { return left * 10i32 + right; }
export function pick(left: i32, right: i32): i32 { return right; }
"#;
    installed(
        "o15_arithmetic::arithmetic_local_reads_charge_their_module_path",
        |request| {
            fs::write(
                &request.entry,
                format!(
                    "{IMPORTS}\nimport short::mix as shortMix;\n\
                     import arithmetic_read_from_a_module_with_a_much_longer_path::mix as longMix;\n"
                ),
            )
            .unwrap();
            for file in [SHORT, LONG] {
                fs::write(request.entry.with_file_name(file), BODIES).unwrap();
            }
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let work = |path: &str, name: &str| {
                    let module = graph
                        .modules()
                        .iter()
                        .find(|m| m.key.origin == Origin::Application && m.key.path == path)
                        .unwrap();
                    let target = lower_body(
                        &receipt,
                        module,
                        &named(module, name).id,
                        AnalysisLimits::default(),
                    )
                    .unwrap();
                    assert_eq!(target.completion(), Completion::Checked, "{path} {name}");
                    target.statistics().work as isize
                };
                let arithmetic = work(LONG, "mix") - work(SHORT, "mix");
                let control = work(LONG, "pick") - work(SHORT, "pick");
                // Two local reads, each two probes of a key ending in the path.
                assert_eq!(
                    arithmetic - control,
                    4 * (LONG.len() - SHORT.len()) as isize
                );
            }
        },
    );
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
function mix(left: i32, right: i32): i32 { return (((left) * ((0x11i32)))) + ((right)); }
function otherWidth(left: i64, right: i64): i64 { return left * 10i64 + right; }
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
fn unsupported_arithmetic_keeps_pre_arithmetic_caps_and_sites() {
    probe(
        "o15_arithmetic::unsupported_arithmetic_keeps_pre_arithmetic_caps_and_sites",
        r#"
function direct(left: i32, right: i32): i32 { return left * right; }
function computed(left: i32, right: i32): i32 { return left * (4i32 + 6i32) + right; }
function grouped(left: i32, right: i32): i32 {
    return (((left * (((((((((4i32 + 6i32)))))))))))) + right;
}
function scoped(left: i32, gate: bool): i32 {
    if (gate) {
        let right = 3i32;
        return left * (4i32 + 6i32) + right;
    }
    return left;
}
function screened(xs: List<i32>, left: i32, right: i32): i32 {
    for (let item of xs) { return left * (4i32 + 6i32) + right; }
    return 0i32;
}
"#,
        |_, module, receipt| {
            // Exact results measured at a2d8e05, before arithmetic lowering.
            // Metering rule 11 (ri-785.9): the graph was 191,033/4,151/5; the
            // i32 equality source root adds 316 work and 134 entries.
            assert_eq!(
                analyze_ownership(receipt, caps(statistics(191033, 4151, 5)).0).completion(),
                GraphCompletion::ResourceLimit
            );
            let graph_stats = statistics(191349, 4285, 5);
            let (exact, short) = caps(graph_stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert_eq!(graph.statistics(), graph_stats);
            for cap in short {
                assert_eq!(
                    analyze_ownership(receipt, cap).completion(),
                    GraphCompletion::ResourceLimit
                );
            }
            for (name, work, entries, depth, expected) in [
                ("direct", 2015, 101, 3, "left * right"),
                ("computed", 2015, 101, 3, "left * (4i32 + 6i32) + right"),
                (
                    "grouped",
                    2015,
                    101,
                    3,
                    "(((left * (((((((((4i32 + 6i32)))))))))))) + right",
                ),
                ("scoped", 2037, 113, 5, "left * (4i32 + 6i32) + right"),
                (
                    "screened",
                    2042,
                    106,
                    3,
                    "for (let item of xs) { return left * (4i32 + 6i32) + right; }",
                ),
            ] {
                let decl = named(module, name);
                let query = |cap| lower_body(receipt, module, &decl.id, cap).unwrap();
                let stats = statistics(work, entries, depth);
                let (exact, short) = caps(stats);
                let target = query(exact);
                assert_eq!(target.completion(), Completion::Incomplete, "{name}");
                assert_eq!(target.statistics(), stats, "{name}");
                assert!(target.mir().is_none() && target.findings().is_empty());
                assert_eq!(
                    target.blocked_at().unwrap().span.bytes(),
                    expected.as_bytes()
                );
                let root = graph_root(&graph, module, decl.node);
                assert_eq!(root.ordinary_completion(), Completion::Incomplete, "{name}");
                assert!(root.ordinary_mir().is_none());
                assert_eq!(root.blocked_at().unwrap().span.bytes(), expected.as_bytes());
                for cap in short {
                    let limited = query(cap);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
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

fn statistics(work: usize, entries: usize, max_depth: usize) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
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
