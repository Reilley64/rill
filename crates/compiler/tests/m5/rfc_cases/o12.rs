//! RFC 0007 O12 (ri-2s0.8): a reference to a call result lives through its
//! statement and no longer, and a direct borrow of an uninterpolated string
//! literal has program-lifetime static backing (RFC 0006). The O12 row above
//! pins the outcome and caps of the RFC's own module; these tests pin the
//! operation sequences, the old caps (metering rule 11), the admitted forms
//! and the unsupported siblings at their parent caps (rule 2).
use super::*;
use compiler::analysis::ownership::{BorrowedMemberBinding, Projection};

pub(super) struct Case {
    pub(super) text: &'static str,
    pub(super) typed_stats: Triple,
    pub(super) graph: (GraphCompletion, Triple),
    pub(super) roots: &'static [Root],
}

fn check(test: &str, cases: &[Case]) {
    installed(test, |request| check_cases(&request, cases));
}

pub(super) fn check_cases(request: &InstalledRequest, cases: &[Case]) {
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
            assert_eq!(
                typed.statistics(),
                statistics(case.typed_stats),
                "{context}"
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
}

pub(super) fn application(graph: &loader::Graph) -> &loader::Module {
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
            Operation::Initialize(value) => events.push(format!("init {}", place(value))),
            Operation::Copy {
                from,
                into: Some(into),
            } => events.push(format!("copy {} -> {}", place(from), place(into))),
            Operation::CallIntoTemporary {
                arguments, into, ..
            } => events.push(format!(
                "call [{}] -> {}",
                arguments.iter().map(place).collect::<Vec<_>>().join(", "),
                place(into)
            )),
            Operation::Borrow {
                source,
                holder,
                exclusive: false,
            } => events.push(format!("borrow {} -> {}", place(source), place(holder))),
            Operation::BorrowStatic { holder } => {
                events.push(format!("borrow static -> {}", place(holder)))
            }
            Operation::BindBorrowedMember {
                descriptor,
                projection: Projection::Field(field),
                into,
                binding: BorrowedMemberBinding::CopyScalar,
            } => events.push(format!(
                "read {}.{} -> {}",
                place(descriptor),
                field.name.as_deref().unwrap(),
                place(into)
            )),
            Operation::Export(value) => events.push(format!("export {}", place(value))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    events.push(match action {
                        Obligation::EndStorage(value) => format!("end {}", place(value)),
                        other => panic!("unexpected cleanup {other:?}"),
                    });
                }
            }
            other => panic!("unexpected operation {other:?}"),
        }
    }
    events
}

const SEQUENCES: &str = r#"
type Point = { x: i32; y: i32 };
function shifted(point: Point): Point { return point; }
function within(): i32 { return (&shifted(Point { x: 1, y: 2 })).x; }
function unused(): void { let view = &shifted(Point { x: 1, y: 2 }); }
function literal(): &string { return &"static"; }
function bound(): &string { let view = &"static"; return view; }
"#;

/// The temporary's backing and its descriptor end where their statement
/// ends, descriptor first. A static literal view has no owner: nothing ends
/// with it but the named holder, and no owned string is created or disposed.
#[test]
fn borrowed_temporary_ends_with_its_statement_and_static_view_has_no_owner() {
    let producer = r#"shifted(Point { x: 1, y: 2 })"#;
    let temporary = |event: &str| event.replace("TEMP", producer);
    let fields = [
        "init Point { x: 1, y: 2 }.x",
        "init Point { x: 1, y: 2 }.y",
        "init Point { x: 1, y: 2 }",
    ];
    let borrowed: Vec<(&str, Vec<String>)> = vec![
        (
            "within",
            fields
                .iter()
                .map(|event| event.to_string())
                .chain(
                    [
                        "call [Point { x: 1, y: 2 }] -> TEMP",
                        "borrow TEMP -> &TEMP",
                        "read &TEMP.x -> (&TEMP).x",
                        "end &TEMP",
                        "end TEMP.y",
                        "end TEMP.x",
                        "end TEMP",
                        "export (&TEMP).x",
                    ]
                    .map(temporary),
                )
                .collect(),
        ),
        (
            "unused",
            fields
                .iter()
                .map(|event| event.to_string())
                .chain(
                    [
                        "call [Point { x: 1, y: 2 }] -> TEMP",
                        "borrow TEMP -> view",
                        "end TEMP.y",
                        "end TEMP.x",
                        "end TEMP",
                        "end view",
                    ]
                    .map(temporary),
                )
                .collect(),
        ),
        (
            "literal",
            vec![
                r#"borrow static -> &"static""#.to_string(),
                r#"export &"static""#.to_string(),
            ],
        ),
        (
            "bound",
            vec![
                "borrow static -> view".to_string(),
                "copy view -> view".to_string(),
                "export view".to_string(),
                "end view".to_string(),
            ],
        ),
    ];
    installed(
        "rfc_cases::o12::borrowed_temporary_ends_with_its_statement_and_static_view_has_no_owner",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{SEQUENCES}")).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let module = application(&graph);
                let whole = analyze_ownership(&receipt, AnalysisLimits::default());
                for (name, expected) in &borrowed {
                    let declaration = module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.id.kind == DeclarationKind::Function
                                && d.name.as_deref() == Some(*name)
                        })
                        .unwrap();
                    let targeted =
                        lower_body(&receipt, module, &declaration.id, AnalysisLimits::default())
                            .unwrap();
                    assert_eq!(targeted.completion(), Completion::Checked, "{name}");
                    assert!(targeted.findings().is_empty(), "{name}");
                    assert_eq!(&sequence(targeted.mir().unwrap()), expected, "{name}");
                    // The ordinary graph root holds the same sequence (rule 9).
                    let root = whole
                        .roots()
                        .iter()
                        .find(|r| {
                            r.site().module == &module.key && r.site().node == declaration.node
                        })
                        .unwrap();
                    assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                    assert_eq!(&sequence(root.ordinary_mir().unwrap()), expected, "{name}");
                }
            }
        },
    );
}

/// Metering rule 11, the ticket's mandated flip (ri-2s0.8): the O12 caps at
/// which `within`, `stored` and `literal` were `Incomplete`, and the
/// whole-graph cap, now give `ResourceLimit`. The O12 row holds the new caps
/// and `m5-acceptance/ri-2s0.8.md` the reason.
#[test]
fn incomplete_borrowed_temporary_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o12::incomplete_borrowed_temporary_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O12[0].text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let module = application(&graph);
                for (name, old) in [
                    ("within", (1899, 39, 3)),
                    ("stored", (1932, 52, 4)),
                    ("literal", (1900, 39, 4)),
                ] {
                    let declaration = module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.id.kind == DeclarationKind::Function
                                && d.name.as_deref() == Some(name)
                        })
                        .unwrap();
                    let (old, _) = limits(statistics(old));
                    let limited = lower_body(&receipt, module, &declaration.id, old).unwrap();
                    crate::limit::assert_body_limit(&limited);
                }
                let (old, _) = limits(statistics((188807, 4385, 5)));
                let limited = analyze_ownership(&receipt, old);
                crate::limit::assert_graph_limit(&limited);
            }
        },
    );
}

/// ri-1up, metering rule 11 (ticket's mandated B1 flip, chief of staff
/// ruling 2026-11-24 07:23 AEDT): a scalar field read through a reference to
/// a record is now lowered, so `viewField` is no longer `Incomplete` at
/// `view.x` (1,986 / 86 / 3, parent `4f7a8032`): the borrowed temporary is
/// released while `view` is still read, R0203 as for `stored`. The old cap
/// gives `ResourceLimit`, with no MIR and no finding.
#[test]
fn read_through_a_stored_view_of_a_temporary_rejects_its_release() {
    const VIEW: &str = r####"
type Point = { x: i32; y: i32 };
function shifted(point: Point): Point { return point; }
function viewField(): i32 { let view = &shifted(Point { x: 1, y: 2 }); return view.x; }
"####;
    installed(
        "rfc_cases::o12::read_through_a_stored_view_of_a_temporary_rejects_its_release",
        |request| {
            check_cases(
                &request,
                &[Case {
                    text: VIEW,
                    typed_stats: (325063, 48462, 12),
                    graph: (GraphCompletion::SourceRejected, (188588, 4666, 7)),
                    roots: &[
                        Root {
                            name: "shifted",
                            want: Want::Checked,
                            graph: None,
                            stats: (2778, 237, 3),
                        },
                        Root {
                            name: "viewField",
                            want: Want::Rejected {
                                code: "R0203",
                                site: r####"let view = &shifted(Point { x: 1, y: 2 });"####,
                                related: r####"&shifted(Point { x: 1, y: 2 })"####,
                            },
                            graph: None,
                            stats: (2971, 348, 7),
                        },
                    ],
                }],
            );
            // Metering rule 11 (ri-px1.11.4, ST-3): `view.x` probes `locals`
            // through `charge_key`; the parent caps (`eabe14ad`) give `ResourceLimit`.
            super::list_contents::old_root_caps(
                &request,
                &[(VIEW, &[("viewField", (2962, 348, 7))], (188579, 4666, 7))],
            );
            fs::write(&request.entry, format!("{IMPORTS}\n{VIEW}")).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let module = application(&graph);
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function
                            && d.name.as_deref() == Some("viewField")
                    })
                    .unwrap();
                let (old, _) = limits(statistics((1986, 86, 3)));
                let limited = lower_body(&receipt, module, &declaration.id, old).unwrap();
                crate::limit::assert_body_limit(&limited);
            }
        },
    );
}

/// Grouped, scoped and loop-wrapped forms, a second field, a parameter
/// argument, a never-read descriptor, the other literal spellings, and a
/// dead and a feasible store: each measured on the candidate.
#[test]
fn admitted_forms_check_and_a_feasible_store_rejects() {
    check(
        "rfc_cases::o12::admitted_forms_check_and_a_feasible_store_rejects",
        &[Case {
            text: r####"
type Point = { x: i32; y: i32 };
function shifted(point: Point): Point { return point; }
function bare(): void { let view = &shifted(Point { x: 1, y: 2 }); }
function grouped(): i32 { return (((&((shifted(Point { x: 1, y: 2 })))))).x; }
function groupedLiteral(): &string { return &(("static")); }
function viaParam(p: Point): i32 { return (&shifted(p)).x; }
function second(): i32 { return (&shifted(Point { x: 1, y: 2 })).y; }
function looped(again: bool): i32 { while (again) { return (&shifted(Point { x: 1, y: 2 })).x; } return 0i32; }
function loopedLiteral(again: bool): &string { while (again) { return &"static"; } return &"other"; }
function deadStore(): void { if (false) { let view = &shifted(Point { x: 1, y: 2 }); let later = view; } }
function liveStore(flag: bool): void { if (flag) { let view = &shifted(Point { x: 1, y: 2 }); let later = view; } }
function rawLiteral(): &string { return &r#"raw"#; }
function backtick(): &string { return &`tick`; }
function boundLiteral(): &string { let view = &"static"; return view; }
function literal(): &string { return &"static"; }
"####,
            typed_stats: (345401, 50550, 14),
            graph: (GraphCompletion::SourceRejected, (206060, 7452, 11)),
            roots: &[
                Root {
                    name: "shifted",
                    want: Want::Checked,
                    graph: None,
                    stats: (3030, 363, 3),
                },
                Root {
                    name: "bare",
                    want: Want::Checked,
                    graph: None,
                    stats: (6285, 845, 7),
                },
                Root {
                    name: "grouped",
                    want: Want::Checked,
                    graph: None,
                    stats: (6358, 881, 11),
                },
                Root {
                    name: "groupedLiteral",
                    want: Want::Checked,
                    graph: None,
                    stats: (2171, 176, 6),
                },
                Root {
                    name: "viaParam",
                    want: Want::Checked,
                    graph: None,
                    stats: (6794, 976, 8),
                },
                Root {
                    name: "second",
                    want: Want::Checked,
                    graph: None,
                    stats: (6355, 881, 9),
                },
                Root {
                    name: "looped",
                    want: Want::Checked,
                    graph: None,
                    stats: (6703, 1084, 11),
                },
                Root {
                    name: "loopedLiteral",
                    want: Want::Checked,
                    graph: None,
                    stats: (2355, 278, 6),
                },
                Root {
                    name: "deadStore",
                    want: Want::Checked,
                    graph: None,
                    stats: (2148, 167, 4),
                },
                Root {
                    name: "liveStore",
                    want: Want::Rejected {
                        code: "R0203",
                        site: r####"let view = &shifted(Point { x: 1, y: 2 });"####,
                        related: r####"&shifted(Point { x: 1, y: 2 })"####,
                    },
                    graph: None,
                    stats: (3415, 598, 9),
                },
                Root {
                    name: "rawLiteral",
                    want: Want::Checked,
                    graph: None,
                    stats: (2169, 176, 4),
                },
                Root {
                    name: "backtick",
                    want: Want::Checked,
                    graph: None,
                    stats: (2170, 176, 4),
                },
                Root {
                    name: "boundLiteral",
                    want: Want::Checked,
                    graph: None,
                    stats: (2262, 222, 4),
                },
                Root {
                    name: "literal",
                    want: Want::Checked,
                    graph: None,
                    stats: (2169, 176, 4),
                },
            ],
        }],
    );
}

/// Metering rule 2: the nearest unsupported siblings stay `Incomplete` at
/// the caps measured at `bc9bfdd5`, with the blocker span of each. Removing
/// `viewField` (a read through a stored view; it left this list for ri-1up,
/// see
/// `read_through_a_stored_view_of_a_temporary_rejects_its_release`) lowers
/// every root's cost by a few units (fewer declarations to search), so the
/// module's caps were re-measured at `bc9bfdd5` without it. `owner` stays
/// R0202 (RFC 0006, a named owner gains no static duration).
#[test]
fn unsupported_siblings_keep_their_parent_caps() {
    check(
        "rfc_cases::o12::unsupported_siblings_keep_their_parent_caps",
        &[Case {
            text: r####"
type Point = { x: i32; y: i32 };
function shifted(point: Point): Point { return point; }
function copyOut(): Point { return *(&shifted(Point { x: 1, y: 2 })); }
function bound(): i32 { let x = (&shifted(Point { x: 1, y: 2 })).x; return x; }
function interpolated(n: i32): &string { return &`a${n}`; }
function number(): &i32 { return &1i32; }
function owner(): &string { let text = "x"; return &text; }
function flag(b: bool): &string { return &`a${b}`; }
"####,
            typed_stats: (333602, 49443, 12),
            graph: (GraphCompletion::SourceRejected, (191338, 4565, 5)),
            roots: &[
                Root {
                    name: "shifted",
                    want: Want::Checked,
                    graph: None,
                    stats: (2862, 279, 3),
                },
                Root {
                    name: "copyOut",
                    want: Want::Incomplete {
                        blocker: r####"*(&shifted(Point { x: 1, y: 2 }))"####,
                    },
                    graph: None,
                    stats: (2306, 124, 3),
                },
                Root {
                    name: "bound",
                    want: Want::Incomplete {
                        blocker: r####"(&shifted(Point { x: 1, y: 2 })).x"####,
                    },
                    graph: None,
                    stats: (1972, 75, 3),
                },
                Root {
                    name: "interpolated",
                    want: Want::Incomplete {
                        blocker: r####"`a${n}`"####,
                    },
                    graph: None,
                    stats: (1958, 71, 4),
                },
                Root {
                    name: "number",
                    want: Want::Incomplete {
                        blocker: r####"1i32"####,
                    },
                    graph: None,
                    stats: (1952, 65, 4),
                },
                Root {
                    name: "owner",
                    want: Want::Rejected {
                        code: "R0202",
                        site: r####"return &text;"####,
                        related: r####"text"####,
                    },
                    graph: None,
                    stats: (2062, 126, 4),
                },
                Root {
                    name: "flag",
                    want: Want::Incomplete {
                        blocker: r####"`a${b}`"####,
                    },
                    graph: None,
                    stats: (1958, 71, 4),
                },
            ],
        }],
    );
}

#[test]
fn text_record_results_stay_incomplete() {
    const TEXT: &str = r####"
type Named = { x: i32; label: string };
function named(): Named { return Named { x: 1, label: "a" }; }
function textRead(): i32 { return (&named()).x; }
function textBind(): void { let view = &named(); }
"####;
    installed(
        "rfc_cases::o12::text_record_results_stay_incomplete",
        |request| {
            check_cases(
                &request,
                &[Case {
                    text: TEXT,
                    typed_stats: (326067, 48483, 12),
                    graph: (GraphCompletion::Incomplete, (187421, 4277, 5)),
                    roots: &[
                        Root {
                            name: "named",
                            want: Want::Checked,
                            graph: None,
                            stats: (2373, 159, 4),
                        },
                        Root {
                            name: "textRead",
                            want: Want::Incomplete {
                                blocker: r####"(&named()).x"####,
                            },
                            graph: None,
                            stats: (1873, 26, 3),
                        },
                        Root {
                            name: "textBind",
                            want: Want::Incomplete {
                                blocker: r####"named()"####,
                            },
                            graph: None,
                            stats: (1889, 33, 4),
                        },
                    ],
                }],
            );
        },
    );
}

#[test]
fn disposer_record_results_keep_their_parent_caps_and_outcomes() {
    const DISPOSER: &str = r####"
type Tracked = { x: i32 };
implement dispose(value: Tracked): void {}
function make(): Tracked { return Tracked { x: 1i32 }; }
function within(): i32 { return (&make()).x; }
function bound(): void { let v = &make(); }
"####;
    const ALIASED: &str = r####"
import std::core::dispose as release;
type Tracked = { x: i32 };
implement release(value: Tracked): void {}
function make(): Tracked { return Tracked { x: 1i32 }; }
function within(): i32 { return (&make()).x; }
function bound(): void { let v = &make(); }
"####;
    const ROOTS: &[Root] = &[
        Root {
            name: "make",
            want: Want::Checked,
            graph: None,
            stats: (2339, 137, 4),
        },
        Root {
            name: "within",
            want: Want::Incomplete {
                blocker: r####"(&make()).x"####,
            },
            graph: None,
            stats: (1872, 27, 2),
        },
        Root {
            name: "bound",
            want: Want::Incomplete {
                blocker: r####"make()"####,
            },
            graph: None,
            stats: (1887, 34, 4),
        },
    ];
    installed(
        "rfc_cases::o12::disposer_record_results_keep_their_parent_caps_and_outcomes",
        |request| {
            check_cases(
                &request,
                &[
                    Case {
                        text: DISPOSER,
                        typed_stats: (327094, 48571, 12),
                        graph: (GraphCompletion::Incomplete, (187642, 4260, 5)),
                        roots: ROOTS,
                    },
                    Case {
                        text: ALIASED,
                        typed_stats: (327965, 48579, 12),
                        graph: (GraphCompletion::Incomplete, (187643, 4260, 5)),
                        roots: ROOTS,
                    },
                ],
            );
        },
    );
}
