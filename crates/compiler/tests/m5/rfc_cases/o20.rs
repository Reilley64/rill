//! RFC 0007 O20 (ri-w0g): a template over an owned record that holds a
//! `LockGuard<i32>` reads each i32 hole into its own scratch, formats the
//! holes through the primitive Show and releases the guard once, at scope
//! exit. The O20 row above pins its outcome and caps; these tests pin the
//! cleanup sequence, the old caps (metering rule 11), the admitted forms
//! and the unsupported siblings at their parent caps (rule 2).
use super::*;
use compiler::analysis::ownership::DependencyContract;

struct Case {
    text: &'static str,
    typed_stats: Triple,
    graph: (GraphCompletion, Triple),
    roots: &'static [Root],
}

fn check(test: &str, cases: &[Case]) {
    installed(test, |request| {
        for (index, case) in cases.iter().enumerate() {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", case.text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let context = format!("case {index}, test_mode {test_mode}");
                assert_eq!(typed.completion(), TypedCompletion::Checked, "{context}");
                crate::limit::assert_stats(
                    typed.statistics(),
                    statistics(case.typed_stats),
                    &context,
                );
                check_receipt(
                    &context,
                    &typed,
                    application(&graph),
                    case.graph,
                    case.roots,
                );
            }
        }
    });
}

fn application(graph: &loader::Graph) -> &loader::Module {
    graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
        .unwrap()
}

fn place(place: &compiler::analysis::ownership::Place<'_>) -> String {
    let root = String::from_utf8_lossy(place.root.span.bytes()).into_owned();
    match place.field() {
        Some(field) => format!("{root}.{}", field.name.as_deref().unwrap()),
        None => root,
    }
}

/// Every operation on the straight path to the return, one entry per action.
fn sequence(mir: &Mir<'_>) -> Vec<String> {
    let blocks = straight_blocks(mir, mir.entry);
    assert!(matches!(
        blocks.last().unwrap().terminator,
        Terminator::Return
    ));
    let mut events = Vec::new();
    for block in blocks {
        match &block.operation {
            Operation::None => {}
            Operation::Copy {
                from,
                into: Some(into),
            } => events.push(format!("copy {} -> {}", place(from), place(into))),
            Operation::FormatI32Template { holes, into } => events.push(format!(
                "format [{}] -> {}",
                holes.iter().map(place).collect::<Vec<_>>().join(", "),
                place(into)
            )),
            Operation::Export(value) => events.push(format!("export {}", place(value))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    events.push(match action {
                        Obligation::EndStorage(value) => format!("end {}", place(value)),
                        Obligation::ReleaseGuard { operand, .. } => {
                            format!("release {}", place(operand))
                        }
                        other => panic!("unexpected cleanup {other:?}"),
                    });
                }
            }
            other => panic!("unexpected operation {other:?}"),
        }
    }
    events
}

#[test]
fn interpolation_reads_end_before_the_guard_is_released_once() {
    installed(
        "rfc_cases::o20::interpolation_reads_end_before_the_guard_is_released_once",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O20[0].text)).unwrap();
            // Each read ends in the formatting, which initializes the result.
            // The result leaves by the return; the guard is released once,
            // at scope exit, after the result is exported.
            let expected = [
                "copy outer: Outer.value -> outer.value",
                "copy outer: Outer.value -> outer.value",
                "format [outer.value, outer.value] -> `${outer.value}-${outer.value}`",
                "export `${outer.value}-${outer.value}`",
                "end outer: Outer.value",
                "release outer: Outer.lock",
                "end outer: Outer",
            ];
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let module = application(&graph);
                let render = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function
                            && d.name.as_deref() == Some("render")
                    })
                    .unwrap();
                let targeted =
                    lower_body(&receipt, module, &render.id, AnalysisLimits::default()).unwrap();
                assert_eq!(targeted.completion(), Completion::Checked);
                assert_eq!(sequence(targeted.mir().unwrap()), expected);

                let report = analyze_ownership(&receipt, AnalysisLimits::default());
                let root = report
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == render.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked);
                assert_eq!(sequence(root.ordinary_mir().unwrap()), expected);
                let [edge] = root.dependencies() else {
                    panic!("{:?}", root.dependencies())
                };
                assert!(matches!(
                    edge.contract,
                    DependencyContract::ProtectedRelease { .. }
                ));
            }
        },
    );
}

/// Metering rule 11, the ticket's mandated flip (ri-w0g): the caps at which
/// `render` was `Incomplete` at the template, 2,230 / 81 / 3 targeted and
/// 186,364 / 4,176 / 5 for the whole graph, now give `ResourceLimit`. The O20
/// row holds the new caps and `m5-acceptance/ri-w0g.md` the reason.
#[test]
fn incomplete_interpolation_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o20::incomplete_interpolation_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O20[0].text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let module = application(&graph);
                let render = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function
                            && d.name.as_deref() == Some("render")
                    })
                    .unwrap();
                let (old, _) = limits(statistics((2230, 81, 3)));
                let limited = lower_body(&receipt, module, &render.id, old).unwrap();
                crate::limit::assert_body_limit(&limited);
                let (old, _) = limits(statistics((186364, 4176, 5)));
                let limited = analyze_ownership(&receipt, old);
                crate::limit::assert_graph_limit(&limited);
            }
        },
    );
}

/// Grouped holes and a grouped template are transparent: same receipt,
/// deeper than the bare O20 form (depth 4), and one short of their own depth
/// gives `ResourceLimit`. A bound result, an expression statement (whose
/// string is dropped where the statement ends), a loop-wrapped return and a
/// plain local with a literal hole also check.
#[test]
fn admitted_interpolation_forms_check() {
    check(
        "rfc_cases::o20::admitted_interpolation_forms_check",
        &[
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(outer: Outer): string { return `${(outer.value)}-${((outer.value))}`; }
"#,
                typed_stats: (325498, 48721, 12),
                graph: (GraphCompletion::Incomplete, (189588, 4314, 6)),
                roots: &[Root {
                    name: "render",
                    want: Want::Checked,
                    graph: None,
                    stats: (2660, 218, 6),
                }],
            },
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(outer: Outer): string { return (`${outer.value}-${outer.value}`); }
"#,
                typed_stats: (325430, 48684, 12),
                graph: (GraphCompletion::Incomplete, (189461, 4312, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Checked,
                    graph: None,
                    stats: (2653, 216, 5),
                }],
            },
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(outer: Outer): string { let text = `${outer.value}`; return text; }
"#,
                typed_stats: (324520, 48480, 12),
                graph: (GraphCompletion::Incomplete, (189451, 4341, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Checked,
                    graph: None,
                    stats: (2702, 245, 4),
                }],
            },
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(outer: Outer): void { `${outer.value}`; }
"#,
                typed_stats: (324442, 48452, 12),
                graph: (GraphCompletion::Incomplete, (189194, 4271, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Checked,
                    graph: None,
                    stats: (2566, 175, 4),
                }],
            },
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(again: bool, outer: Outer): string { while (again) { return `${outer.value}`; } return "none"; }
"#,
                typed_stats: (324866, 48534, 12),
                graph: (GraphCompletion::Incomplete, (189821, 4416, 6)),
                roots: &[Root {
                    name: "render",
                    want: Want::Checked,
                    graph: None,
                    stats: (2830, 320, 6),
                }],
            },
            Case {
                text: r#"
function render(value: i32): string { return `${value}-${1i32}`; }
"#,
                typed_stats: (323561, 48540, 12),
                graph: (GraphCompletion::Incomplete, (188405, 4217, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Checked,
                    graph: None,
                    stats: (2061, 122, 4),
                }],
            },
        ],
    );
}

/// Rule 2: the nearest unsupported holes (a bool, a string, i32 arithmetic,
/// a call and a field through a shared record view) are screened before the
/// first charge, so each stays `Incomplete` at the template with the
/// targeted and whole-graph caps measured at the parent, 7002855e. They are
/// not re-measured on the candidate.
#[test]
fn unsupported_holes_keep_their_parent_caps() {
    check(
        "rfc_cases::o20::unsupported_holes_keep_their_parent_caps",
        &[
            Case {
                text: r#"
type Flagged = { lock: LockGuard<i32>; ready: bool; }
function render(outer: Flagged): string { return `${outer.ready}`; }
"#,
                typed_stats: (324455, 48454, 12),
                graph: (GraphCompletion::Incomplete, (188904, 4174, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Incomplete {
                        blocker: r####"`${outer.ready}`"####,
                    },
                    graph: None,
                    stats: (2375, 79, 3),
                }],
            },
            Case {
                text: r#"
type Named = { lock: LockGuard<i32>; name: string; }
function render(outer: Named): string { return `${outer.name}`; }
"#,
                typed_stats: (324458, 48454, 12),
                graph: (GraphCompletion::Incomplete, (188904, 4174, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Incomplete {
                        blocker: r####"`${outer.name}`"####,
                    },
                    graph: None,
                    stats: (2375, 79, 3),
                }],
            },
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(outer: Outer): string { return `${outer.value + outer.value}`; }
"#,
                typed_stats: (324854, 48512, 12),
                graph: (GraphCompletion::Incomplete, (189090, 4177, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Incomplete {
                        blocker: r####"`${outer.value + outer.value}`"####,
                    },
                    graph: None,
                    stats: (2381, 82, 3),
                }],
            },
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function twice(value: i32): i32 { return value; }
function render(outer: Outer): string { return `${twice(outer.value)}`; }
"#,
                typed_stats: (326050, 48556, 12),
                graph: (GraphCompletion::Incomplete, (189615, 4251, 5)),
                roots: &[
                    Root {
                        name: "twice",
                        want: Want::Checked,
                        graph: None,
                        stats: (1998, 83, 3),
                    },
                    Root {
                        name: "render",
                        want: Want::Incomplete {
                            blocker: r####"`${twice(outer.value)}`"####,
                        },
                        graph: None,
                        stats: (2389, 86, 3),
                    },
                ],
            },
            Case {
                text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(outer: &Outer): string { return `${outer.value}`; }
"#,
                typed_stats: (324436, 48463, 12),
                graph: (GraphCompletion::Incomplete, (188457, 4116, 5)),
                roots: &[Root {
                    name: "render",
                    want: Want::Incomplete {
                        blocker: r####"`${outer.value}`"####,
                    },
                    graph: None,
                    stats: (1868, 21, 3),
                }],
            },
        ],
    );
}
