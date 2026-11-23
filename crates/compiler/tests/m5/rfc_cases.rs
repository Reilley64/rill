//! RFC 0007 cases O1 to O24, each asserting the outcome the compiler gives today.
//!
//! Complete modules in the RFC (O1 to O6, O15) are used verbatim, as is O8's
//! extension of O2 and the approved O16 source. The RFC states the other cases
//! in prose; their modules here are written for that prose and are marked in
//! `m5-status.md`. A row that changes outcome is a deliberate change: edit the
//! expectation and its row in the status table in the same commit.
//!
//! Every source is loaded in both installed modes. A source with a typed
//! receipt pins its targeted query and ordinary graph root per root and the
//! whole-graph verdict. A source without one pins the typed completion and
//! every blocker or finding the typed report names in the application module.
//! Every pinned result is also replayed at its exact limits and one short on
//! each axis, which must be `ResourceLimit` with no MIR and no findings.
//! These are compiler judgments over MIR, not Rill execution.
use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{GraphCompletion, analyze_ownership, lower_root};
use compiler::analysis::scalars::{BodyOwner, Site};
use compiler::analysis::typed::TypedCompletion;
use compiler::loader::DeclarationKind;

type Triple = (usize, usize, usize);

#[derive(Clone, Copy, Debug)]
enum Want {
    Checked,
    Rejected {
        code: &'static str,
        site: &'static str,
        related: &'static str,
    },
    Incomplete {
        blocker: &'static str,
    },
}

/// One function, or `expect` for the module's expectation root.
struct Root {
    name: &'static str,
    want: Want,
    /// The ordinary graph root when it differs from the targeted query.
    graph: Option<Want>,
    stats: Triple,
}

enum Outcome {
    Receipt {
        typed_stats: Triple,
        graph: (GraphCompletion, Triple),
        /// The whole graph's exact cap before the i32 equality source root
        /// (ri-785.9, metering rule 11); it now gives `ResourceLimit`.
        pre_equality_graph: Triple,
        roots: &'static [Root],
    },
    Typed {
        completion: TypedCompletion,
        signals: &'static [&'static str],
        stats: Triple,
    },
}

struct Source {
    text: &'static str,
    outcome: Outcome,
}

const IMPORTS: &str = r#"
            import std::{List, Result, Err};
            import std::channel::{Receiver, Sender};
            import std::convert::{into, tryInto};
            import std::error::formatError;
            import std::iter::iterate;
            import std::mutex::LockGuard;
            import std::number::ParseError;
            import std::option::{Option, map as mapOption};
            import std::result::{isOk, map as mapResult};
            import std::shared::Shared;
    "#;

fn run(test: &str, sources: &[Source]) {
    installed(test, |request| {
        for (index, source) in sources.iter().enumerate() {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", source.text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let module = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                    .unwrap();
                let context = format!("source {index}, test_mode {test_mode}");
                match &source.outcome {
                    Outcome::Typed {
                        completion,
                        signals,
                        stats,
                    } => check_typed(
                        &context,
                        &graph,
                        &typed,
                        module,
                        *completion,
                        signals,
                        *stats,
                    ),
                    Outcome::Receipt {
                        typed_stats,
                        graph: whole,
                        pre_equality_graph,
                        roots,
                    } => {
                        assert_eq!(typed.completion(), TypedCompletion::Checked, "{context}");
                        assert_eq!(statistics(*typed_stats), typed.statistics(), "{context}");
                        check_receipt(&context, &typed, module, *whole, roots);
                        let receipt = typed.receipt().unwrap();
                        let (old, _) = limits(statistics(*pre_equality_graph));
                        assert_eq!(
                            analyze_ownership(&receipt, old).completion(),
                            GraphCompletion::ResourceLimit,
                            "{context}"
                        );
                    }
                }
            }
        }
    });
}

fn statistics((work, entries, max_depth): Triple) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

/// The exact limits and one short on each axis. A zero axis has nothing to cut.
fn limits(stats: Statistics) -> (AnalysisLimits, Vec<AnalysisLimits>) {
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let mut short = Vec::new();
    if stats.work > 0 {
        short.push(AnalysisLimits {
            max_work: stats.work - 1,
            ..exact
        });
    }
    if stats.entries > 0 {
        short.push(AnalysisLimits {
            max_entries: stats.entries - 1,
            ..exact
        });
    }
    if stats.max_depth > 0 {
        short.push(AnalysisLimits {
            max_depth: stats.max_depth - 1,
            ..exact
        });
    }
    (exact, short)
}

fn text(site: &Site<'_>) -> String {
    site.span.bytes().escape_ascii().to_string()
}

fn signals(typed: &TypedReport<'_>, module: &loader::Module) -> Vec<String> {
    let mut found = Vec::new();
    for finding in typed.bodies().findings() {
        if finding.site.module == &module.key {
            let code = finding.kind.code().map(str::to_string);
            found.push(format!(
                "finding {} @ {}",
                code.unwrap_or_else(|| format!("{:?}", finding.kind)),
                text(&finding.site)
            ));
        }
    }
    for obligation in typed.bodies().obligations() {
        if obligation.site.module == &module.key {
            found.push(format!(
                "obligation {:?} @ {}",
                obligation.reason,
                text(&obligation.site)
            ));
        }
    }
    for pending in typed
        .result_use()
        .into_iter()
        .flat_map(|uses| uses.pending())
    {
        if pending.site.module == &module.key {
            found.push(format!(
                "pending {:?} @ {}",
                pending.blocker,
                text(&pending.site)
            ));
        }
    }
    for overlap in typed.ownership_overlaps() {
        found.push(format!(
            "overlap {} @ {}",
            overlap.code(),
            overlap.implementation.span.bytes().escape_ascii()
        ));
    }
    found
}

fn check_typed(
    context: &str,
    graph: &loader::Graph,
    typed: &TypedReport<'_>,
    module: &loader::Module,
    completion: TypedCompletion,
    expected: &[&str],
    stats: Triple,
) {
    assert_eq!(typed.completion(), completion, "{context}");
    assert!(typed.receipt().is_none(), "{context}");
    assert_eq!(signals(typed, module), expected, "{context}");
    assert_eq!(typed.statistics(), statistics(stats), "{context}");
    let (exact, short) = limits(typed.statistics());
    let replay = analyze_typed(graph, exact);
    assert_eq!(replay.completion(), completion, "{context}");
    assert_eq!(replay.statistics(), typed.statistics(), "{context}");
    for short in short {
        let limited = analyze_typed(graph, short);
        assert_eq!(
            limited.completion(),
            TypedCompletion::ResourceLimit,
            "{context}"
        );
        assert!(limited.receipt().is_none(), "{context}");
    }
}

fn check_receipt<'g>(
    context: &str,
    typed: &TypedReport<'g>,
    module: &'g loader::Module,
    (whole, whole_stats): (GraphCompletion, Triple),
    roots: &[Root],
) {
    let receipt = typed.receipt().unwrap();
    let ownership = analyze_ownership(&receipt, AnalysisLimits::default());
    assert_eq!(ownership.completion(), whole, "{context}");
    assert_eq!(ownership.statistics(), statistics(whole_stats), "{context}");
    let (exact, short) = limits(ownership.statistics());
    let replay = analyze_ownership(&receipt, exact);
    assert_eq!(replay.completion(), whole, "{context}");
    assert_eq!(replay.statistics(), ownership.statistics(), "{context}");
    for short in short {
        assert_eq!(
            analyze_ownership(&receipt, short).completion(),
            GraphCompletion::ResourceLimit,
            "{context}"
        );
    }

    let mut rejected = Vec::new();
    for root in roots {
        let context = format!("{context}, {}", root.name);
        let declaration = module
            .declarations
            .iter()
            .find(|d| match root.name {
                "expect" => d.id.kind == DeclarationKind::Expectation,
                name => d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some(name),
            })
            .unwrap();
        let query = |limits| {
            if root.name == "expect" {
                lower_root(
                    &receipt,
                    module,
                    BodyOwner::Declaration(&declaration.id),
                    limits,
                )
            } else {
                lower_body(&receipt, module, &declaration.id, limits)
            }
            .unwrap()
        };
        let target = query(AnalysisLimits::default());
        assert_eq!(target.statistics(), statistics(root.stats), "{context}");
        let findings: Vec<_> = target
            .findings()
            .iter()
            .map(|f| (f.code(), text(&f.site), text(&f.related)))
            .collect();
        match root.want {
            Want::Checked => {
                assert_eq!(target.completion(), Completion::Checked, "{context}");
                assert!(target.mir().is_some_and(|mir| !mir.blocks.is_empty()));
                assert!(findings.is_empty() && target.blocked_at().is_none());
            }
            Want::Rejected {
                code,
                site,
                related,
            } => {
                assert_eq!(target.completion(), Completion::SourceRejected, "{context}");
                assert!(target.mir().is_none() && target.blocked_at().is_none());
                assert_eq!(findings, [(code, site.to_string(), related.to_string())]);
            }
            Want::Incomplete { blocker } => {
                assert_eq!(target.completion(), Completion::Incomplete, "{context}");
                assert!(target.mir().is_none() && findings.is_empty());
                assert_eq!(text(&target.blocked_at().unwrap()), blocker, "{context}");
            }
        }
        let (exact, short) = limits(target.statistics());
        let replay = query(exact);
        assert_eq!(replay.completion(), target.completion(), "{context}");
        assert_eq!(replay.statistics(), target.statistics(), "{context}");
        for short in short {
            let limited = query(short);
            assert_eq!(limited.completion(), Completion::ResourceLimit, "{context}");
            assert!(limited.mir().is_none() && limited.findings().is_empty());
        }

        let graph_root = ownership
            .roots()
            .iter()
            .find(|r| r.site().module == &module.key && r.site().node == declaration.node)
            .unwrap();
        match root.graph.unwrap_or(root.want) {
            Want::Checked => {
                assert_eq!(
                    graph_root.ordinary_completion(),
                    Completion::Checked,
                    "{context}"
                );
                assert!(
                    graph_root
                        .ordinary_mir()
                        .is_some_and(|mir| !mir.blocks.is_empty())
                );
                assert!(graph_root.blocked_at().is_none());
            }
            Want::Rejected {
                code,
                site,
                related,
            } => {
                assert_eq!(
                    graph_root.ordinary_completion(),
                    Completion::SourceRejected,
                    "{context}"
                );
                assert!(graph_root.ordinary_mir().is_none() && graph_root.blocked_at().is_none());
                rejected.push((code, site.to_string(), related.to_string()));
            }
            Want::Incomplete { blocker } => {
                assert_eq!(
                    graph_root.ordinary_completion(),
                    Completion::Incomplete,
                    "{context}"
                );
                assert!(graph_root.ordinary_mir().is_none());
                assert_eq!(
                    text(&graph_root.blocked_at().unwrap()),
                    blocker,
                    "{context}"
                );
            }
        }
    }
    // The graph keeps one finding per rejected root in this module.
    let mut recorded: Vec<_> = ownership
        .findings()
        .iter()
        .filter(|f| f.site.module == &module.key)
        .map(|f| (f.code(), text(&f.site), text(&f.related)))
        .collect();
    recorded.sort();
    rejected.sort();
    assert_eq!(recorded, rejected, "{context}");
}

const O1: &[Source] = &[Source {
    text: r#"
type Point = { x: i32; y: i32 };

function increment(value: &mut i32): void {
  *value += 1i32;
}
function shifted(point: Point): Point {
  let mut moved = point;
  increment(&mut moved.x);
  return moved;
}
expect shifted(Point { x: 2, y: 3 }).x == 3i32;
"#,
    outcome: Outcome::Typed {
        completion: TypedCompletion::Incomplete,
        signals: &[r####"pending Storage @ *value += 1i32;"####],
        stats: (325375, 47829, 12),
    },
}];

const O2: &[Source] = &[Source {
    text: r#"
type Point = { x: i32; y: i32 };
function remember(slot: &mut &Point, source: &Point): void {
  *slot = source;
}
function selected(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return view.x;
}
expect selected() == 9i32;
"#,
    outcome: Outcome::Typed {
        completion: TypedCompletion::Incomplete,
        signals: &[
            r####"pending Storage @ *slot = source;"####,
            r####"pending Storage @ &mut view"####,
        ],
        stats: (325943, 47994, 12),
    },
}];

const O3: &[Source] = &[Source {
    text: r#"
function identity<T>(value: T): T { return value; }
function carried(): i32 {
  let owner = 42i32;
  let view = identity(&owner);
  return *view;
}
expect carried() == 42i32;
"#,
    outcome: Outcome::Receipt {
        typed_stats: (324039, 47795, 12),
        graph: (GraphCompletion::Incomplete, (186982, 4212, 5)),
        pre_equality_graph: (186482, 4013, 3),
        roots: &[
            Root {
                name: "identity",
                want: Want::Incomplete {
                    blocker: r####"value: T"####,
                },
                graph: Some(Want::Incomplete {
                    blocker: r####"function identity<T>(value: T): T { return value; }"####,
                }),
                stats: (1863, 23, 0),
            },
            Root {
                name: "carried",
                want: Want::Incomplete {
                    blocker: r####"identity(&owner)"####,
                },
                graph: None,
                stats: (1901, 40, 3),
            },
            Root {
                name: "expect",
                // Since ri-785.9 the comparison lowers, and the expectation
                // stops at the blocker of the `carried` body it calls.
                want: Want::Incomplete {
                    blocker: r####"identity(&owner)"####,
                },
                graph: None,
                stats: (3899, 127, 3),
            },
        ],
    },
}];

const O4: &[Source] = &[Source {
    text: r#"
function invalidMove(): string {
  let first = "owned";
  let second = first;
  return first;
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (322086, 47538, 12),
        graph: (GraphCompletion::SourceRejected, (185884, 4186, 5)),
        pre_equality_graph: (185568, 4052, 3),
        roots: &[Root {
            name: "invalidMove",
            want: Want::Rejected {
                code: "R0200",
                site: r####"first"####,
                related: r####"first"####,
            },
            graph: None,
            stats: (1992, 90, 3),
        }],
    },
}];

const O5: &[Source] = &[Source {
    text: r#"
type Pair = { left: i32; right: i32 };
function separate(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let right = &mut pair.right;
  *left += 1i32;
  *right += 2i32;
  return pair.left + pair.right;
}
expect separate() == 6i32;
"#,
    outcome: Outcome::Receipt {
        typed_stats: (325745, 48049, 12),
        graph: (GraphCompletion::Incomplete, (188006, 4283, 5)),
        pre_equality_graph: (187506, 4084, 3),
        roots: &[
            Root {
                name: "separate",
                want: Want::Incomplete {
                    blocker: r####"pair.left + pair.right"####,
                },
                graph: None,
                stats: (2281, 116, 3),
            },
            Root {
                name: "expect",
                // Since ri-785.9 the comparison lowers, and the expectation
                // stops at the blocker of the `separate` body it calls.
                want: Want::Incomplete {
                    blocker: r####"pair.left + pair.right"####,
                },
                graph: None,
                stats: (4305, 216, 3),
            },
        ],
    },
}];

const O6: &[Source] = &[Source {
    text: r#"
function change(value: &mut i32): void { *value += 1i32; }
function twice(value: &mut i32): void {
  change(value);
  change(value);
}
function result(): i32 {
  let mut value = 1i32;
  twice(&mut value);
  return value;
}
expect result() == 3i32;
"#,
    outcome: Outcome::Typed {
        completion: TypedCompletion::Incomplete,
        signals: &[r####"pending Storage @ *value += 1i32;"####],
        stats: (325245, 47754, 12),
    },
}];

const O7: &[Source] = &[Source {
    text: r#"
type Holder = { first: &mut i32; second: &i32 };
function overlap(): i32 {
  let mut value = 1i32;
  let mut holder = Holder { first: &mut value, second: &value };
  *holder.first += 1i32;
  return *holder.second;
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (324564, 47874, 12),
        graph: (GraphCompletion::Incomplete, (187028, 4183, 5)),
        pre_equality_graph: (186712, 4049, 3),
        roots: &[Root {
            name: "overlap",
            want: Want::Incomplete {
                blocker: r####"holder"####,
            },
            graph: None,
            stats: (2233, 88, 1),
        }],
    },
}];

const O8: &[Source] = &[Source {
    text: r#"
type Point = { x: i32; y: i32 };
function remember(slot: &mut &Point, source: &Point): void {
  *slot = source;
}
function selected(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return view.x;
}
expect selected() == 9i32;
function poison(slot: &mut &Point): void {
  let local = Point { x: 1, y: 2 };
  *slot = &local;
}
"#,
    outcome: Outcome::Typed {
        completion: TypedCompletion::Incomplete,
        signals: &[
            r####"pending Storage @ *slot = source;"####,
            r####"pending Storage @ &mut view"####,
            r####"pending Storage @ *slot = &local;"####,
        ],
        stats: (327681, 48227, 12),
    },
}];

const O9: &[Source] = &[Source {
    text: r#"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function forwarded(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
function local(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (329382, 48563, 12),
        graph: (GraphCompletion::Incomplete, (189463, 4237, 5)),
        pre_equality_graph: (189147, 4103, 3),
        roots: &[
            Root {
                name: "forwarded",
                want: Want::Incomplete {
                    blocker: r####"retain(dst, source)"####,
                },
                graph: None,
                stats: (1930, 55, 3),
            },
            Root {
                name: "local",
                want: Want::Incomplete {
                    blocker: r####"retain(dst, &user)"####,
                },
                graph: None,
                stats: (2292, 118, 3),
            },
        ],
    },
}];

const O10: &[Source] = &[
    Source {
        text: r#"
type User = { id: i32 };
function extract(users: List<&User>): Option<&User> {
  for (let user of users) { return Option.Some(user); }
  return Option.None;
}
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"pending Storage @ for (let user of users) { return Option.Some(user); }"####,
            ],
            stats: (323289, 47677, 12),
        },
    },
    Source {
        text: r#"
import std::collection::get;
type User = { id: i32 };
function slot(users: List<&User>): Option<&&User> { return get(&users, 0u64); }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"obligation FamilySelection @ get(&users, 0u64)"####,
                r####"pending TypedInput @ get(&users, 0u64)"####,
                r####"pending TypedInput @ return get(&users, 0u64);"####,
            ],
            stats: (325319, 47875, 12),
        },
    },
];

const O11: &[Source] = &[Source {
    text: r#"
function editAndView<L>(list: L): &i32;
implement editAndView(list: &mut List<i32>): &i32 { while (true) {} }
function conflict(input: List<i32>): i32 {
  let mut owned = input;
  let view = editAndView(&mut owned);
  let whole = &owned;
  return *view;
}
function ordered(input: List<i32>): i32 {
  let mut owned = input;
  let view = editAndView(&mut owned);
  let value = *view;
  let whole = &owned;
  return value;
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (328846, 48424, 12),
        graph: (GraphCompletion::Incomplete, (189575, 4219, 5)),
        pre_equality_graph: (189259, 4085, 4),
        roots: &[
            Root {
                name: "conflict",
                want: Want::Incomplete {
                    blocker: r####"editAndView(&mut owned)"####,
                },
                graph: None,
                stats: (2014, 77, 4),
            },
            Root {
                name: "ordered",
                want: Want::Incomplete {
                    blocker: r####"editAndView(&mut owned)"####,
                },
                graph: None,
                stats: (2032, 83, 4),
            },
        ],
    },
}];

const O12: &[Source] = &[Source {
    text: r#"
type Point = { x: i32; y: i32 };
function shifted(point: Point): Point { return point; }
function within(): i32 { return (&shifted(Point { x: 1, y: 2 })).x; }
function stored(): void {
  let view = &shifted(Point { x: 1, y: 2 });
  let later = view;
}
function literal(): &string { return &"static"; }
"#,
    outcome: Outcome::Receipt {
        typed_stats: (328111, 48084, 12),
        graph: (GraphCompletion::Incomplete, (188807, 4385, 5)),
        pre_equality_graph: (188491, 4251, 4),
        roots: &[
            Root {
                name: "shifted",
                want: Want::Checked,
                graph: None,
                stats: (2810, 253, 3),
            },
            Root {
                name: "within",
                want: Want::Incomplete {
                    blocker: r####"(&shifted(Point { x: 1, y: 2 })).x"####,
                },
                graph: None,
                stats: (1899, 39, 3),
            },
            Root {
                name: "stored",
                want: Want::Incomplete {
                    blocker: r####"shifted(Point { x: 1, y: 2 })"####,
                },
                graph: None,
                stats: (1932, 52, 4),
            },
            Root {
                name: "literal",
                want: Want::Incomplete {
                    blocker: r####"\"static\""####,
                },
                graph: None,
                stats: (1900, 39, 4),
            },
        ],
    },
}];

const O13: &[Source] = &[
    Source {
        text: r#"
function joined(flag: bool, left: string, right: string, seed: &string): void {
  let mut slot = seed;
  if (flag) { slot = &left; } else { slot = &right; }
  dispose(left);
  let last = slot;
}
function loops(flag: bool, left: string, seed: &string): void {
  let mut slot = seed;
  while (flag) { slot = &left; continue; }
  dispose(left);
  let last = slot;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (326945, 48157, 12),
            graph: (GraphCompletion::SourceRejected, (190093, 5031, 6)),
            pre_equality_graph: (189777, 4897, 6),
            roots: &[
                Root {
                    name: "joined",
                    want: Want::Rejected {
                        code: "R0203",
                        site: r####"dispose(left)"####,
                        related: r####"&left"####,
                    },
                    graph: None,
                    stats: (2669, 447, 6),
                },
                Root {
                    name: "loops",
                    want: Want::Rejected {
                        code: "R0203",
                        site: r####"dispose(left)"####,
                        related: r####"&left"####,
                    },
                    graph: None,
                    stats: (2839, 532, 6),
                },
            ],
        },
    },
    Source {
        text: r#"
function remember<D, S>(slot: D, source: S): void;
implement remember(slot: &mut &string, source: &string): void { while (true) {} }
function clear(slot: &mut &string): void { }
function joined(flag: bool, left: string, right: string, seed: &string): void {
  let mut slot = seed;
  if (flag) { remember(&mut slot, &left); } else { remember(&mut slot, &right); }
  clear(&mut slot);
  dispose(left);
  let last = slot;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (332133, 48966, 12),
            graph: (GraphCompletion::Incomplete, (190003, 4236, 5)),
            pre_equality_graph: (189687, 4102, 3),
            roots: &[
                Root {
                    name: "clear",
                    want: Want::Checked,
                    graph: None,
                    stats: (1993, 88, 1),
                },
                Root {
                    name: "joined",
                    want: Want::Incomplete {
                        blocker: r####"&mut slot"####,
                    },
                    graph: None,
                    stats: (2015, 97, 3),
                },
            ],
        },
    },
];

const O14: &[Source] = &[Source {
    text: r#"
import std::core::dispose as release;
type Owner = { text: string; }
implement release(value: Owner): void { dispose(value.text); }
function replace(old: Owner, next: string): Owner {
  let mut slot = old;
  slot = Owner { text: next };
  return slot;
}
function stale(old: Owner, next: string): void {
  let mut slot = old;
  let view = &slot;
  slot = Owner { text: next };
  let last = view;
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (328345, 48121, 12),
        graph: (GraphCompletion::SourceRejected, (193432, 5361, 5)),
        pre_equality_graph: (192262, 5227, 4),
        roots: &[
            Root {
                name: "replace",
                want: Want::Checked,
                graph: None,
                stats: (6922, 882, 4),
            },
            Root {
                name: "stale",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"slot = Owner { text: next };"####,
                    related: r####"&slot"####,
                },
                graph: None,
                stats: (3455, 442, 4),
            },
        ],
    },
}];

const O15: &[Source] = &[Source {
    text: r#"
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
"#,
    outcome: Outcome::Receipt {
        typed_stats: (323681, 47925, 12),
        graph: (GraphCompletion::Incomplete, (188043, 4496, 6)),
        pre_equality_graph: (187543, 4297, 6),
        roots: &[
            Root {
                name: "counter",
                want: Want::Checked,
                graph: None,
                stats: (2562, 329, 6),
            },
            Root {
                name: "expect",
                want: Want::Checked,
                graph: None,
                stats: (4577, 425, 6),
            },
        ],
    },
}];

const O16: &[Source] = &[Source {
    text: r#"
import std::routine::join;
function joined(input: List<i32>): [i32, i32] {
    let numbers = input;
    let [first, second] = join([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
    dispose(numbers);
    return [first, second];
}
function joinedOrder(): bool {
    let input: List<i32> = [13i32, 29i32];
    let pair = joined(input);
    return pair[0] == 13i32 && pair[1] == 29i32;
}
expect joinedOrder();
"#,
    outcome: Outcome::Receipt {
        typed_stats: (434255, 50090, 12),
        graph: (GraphCompletion::Incomplete, (225554, 25180, 10)),
        pre_equality_graph: (225238, 25046, 10),
        roots: &[
            Root {
                name: "joined",
                want: Want::Checked,
                graph: None,
                stats: (32417, 20901, 10),
            },
            Root {
                name: "joinedOrder",
                want: Want::Incomplete {
                    blocker: r####"pair[0] == 13i32 && pair[1] == 29i32"####,
                },
                graph: None,
                stats: (2392, 139, 3),
            },
            Root {
                name: "expect",
                want: Want::Incomplete {
                    blocker: r####"pair[0] == 13i32 && pair[1] == 29i32"####,
                },
                graph: None,
                stats: (4431, 219, 3),
            },
        ],
    },
}];

const O17: &[Source] = &[
    Source {
        text: r#"
function exclusiveCopy(seed: bool): once () => &mut bool {
  let mut flag = seed;
  let job: once () => &mut bool = () => &mut flag;
  return copy(&job);
}
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::SourceRejected,
            signals: &[
                r####"finding R0302 @ copy(&job)"####,
                r####"pending TypedInput @ copy(&job)"####,
                r####"pending TypedInput @ return copy(&job);"####,
            ],
            stats: (324291, 47882, 12),
        },
    },
    Source {
        text: r#"
function repeated(left: string, right: string): [string, string] {
  let job: once () => [string, string] = () => [left, right];
  let _ = job();
  return job();
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (323683, 47770, 12),
            graph: (GraphCompletion::SourceRejected, (187715, 4500, 5)),
            pre_equality_graph: (187399, 4366, 5),
            roots: &[Root {
                name: "repeated",
                want: Want::Rejected {
                    code: "R0200",
                    site: r####"job()"####,
                    related: r####"job()"####,
                },
                graph: None,
                stats: (3102, 404, 5),
            }],
        },
    },
];

const O18: &[Source] = &[Source {
    text: r#"
import std::core::dispose as release;
type Plain = { keep: string; last: string; }
type Custom = { keep: string; last: string; }
implement release(value: Custom): void { let moved = value.keep; }
function plain(value: Plain): string { return value.keep; }
function custom(value: Custom): string { return value.keep; }
"#,
    outcome: Outcome::Receipt {
        typed_stats: (329138, 47934, 12),
        graph: (GraphCompletion::SourceRejected, (187926, 4372, 5)),
        pre_equality_graph: (187610, 4238, 3),
        roots: &[
            Root {
                name: "plain",
                want: Want::Checked,
                graph: None,
                stats: (2388, 160, 3),
            },
            Root {
                name: "custom",
                want: Want::Rejected {
                    code: "R0501",
                    site: r####"value.keep"####,
                    related: r####"value: Custom"####,
                },
                graph: None,
                stats: (2315, 129, 3),
            },
        ],
    },
}];

const O19: &[Source] = &[Source {
    text: r#"
import std::core::dispose as release;
type Holder = { view: &string; }
implement release(value: Holder): void { let seen = value.view; }
function ordered(seed: &string): void {
  let mut holder = Holder { view: seed };
  let source = "later";
  holder = Holder { view: &source };
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (326418, 47921, 12),
        graph: (GraphCompletion::Incomplete, (187190, 4193, 5)),
        pre_equality_graph: (186874, 4059, 3),
        roots: &[Root {
            name: "ordered",
            want: Want::Incomplete {
                blocker: r####"holder"####,
            },
            graph: None,
            stats: (2250, 93, 1),
        }],
    },
}];

const O20: &[Source] = &[Source {
    text: r#"
type Outer = { lock: LockGuard<i32>; value: i32; }
function render(outer: Outer): string { return `${outer.value}-${outer.value}`; }
"#,
    outcome: Outcome::Receipt {
        typed_stats: (325293, 48014, 12),
        graph: (GraphCompletion::Incomplete, (186364, 4176, 5)),
        pre_equality_graph: (186048, 4042, 3),
        roots: &[Root {
            name: "render",
            want: Want::Incomplete {
                blocker: r####"`${outer.value}-${outer.value}`"####,
            },
            graph: None,
            stats: (2230, 81, 3),
        }],
    },
}];

const O21: &[Source] = &[Source {
    text: r#"
import std::core::dispose as release;
import std::mutex::LockGuard;
type Payload = { id: i32; }
implement release(value: LockGuard<Payload>): void { }
"#,
    outcome: Outcome::Typed {
        completion: TypedCompletion::Incomplete,
        signals: &[
            r####"obligation FamilySelection @ { }"####,
            r####"overlap R0500 @ implement release(value: LockGuard<Payload>): void { }"####,
        ],
        stats: (323823, 47493, 12),
    },
}];

const O22: &[Source] = &[
    Source {
        text: r#"
function aliased(owner: string): void {
  let view = &owner;
  let other = view;
  dispose(view);
  dispose(owner);
  let last = other;
}
function ended(owner: string): void {
  let view = &owner;
  let other = view;
  dispose(view);
  let last = other;
}
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"obligation FamilySelection @ dispose(view)"####,
                r####"obligation FamilySelection @ dispose(view)"####,
                r####"pending TypedInput @ dispose(view)"####,
                r####"pending TypedInput @ dispose(view)"####,
            ],
            stats: (325919, 47848, 12),
        },
    },
    Source {
        text: r#"
function aliased(owner: string): void {
  let view = &owner;
  let other = view;
  dispose(owner);
  let last = other;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (322883, 47652, 12),
            graph: (GraphCompletion::SourceRejected, (186359, 4240, 5)),
            pre_equality_graph: (186043, 4106, 4),
            roots: &[Root {
                name: "aliased",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"dispose(owner)"####,
                    related: r####"&owner"####,
                },
                graph: None,
                stats: (2105, 144, 4),
            }],
        },
    },
];

const O23: &[Source] = &[Source {
    text: r#"
function onlyOwned<T>(value: T): T where noBorrow(T) { return value; }
function generic<T>(value: T): T { return onlyOwned(value); }
function concrete(): i32 { return onlyOwned(1i32); }
"#,
    outcome: Outcome::Typed {
        completion: TypedCompletion::SourceRejected,
        signals: &[
            r####"finding R0302 @ onlyOwned(value)"####,
            r####"pending TypedInput @ onlyOwned(value)"####,
            r####"pending TypedInput @ return onlyOwned(value);"####,
        ],
        stats: (325195, 47740, 12),
    },
}];

const O24: &[Source] = &[Source {
    text: r#"
import std::routine::join;
function joined(input: &i32, other: &i32): [&i32, &i32] { return join([() => input, () => other]); }
function released(first: i32, second: i32): i32 {
  let [a, b] = joined(&first, &second);
  let retained = b;
  dispose(first);
  return *retained;
}
function afterUse(first: i32, second: i32): i32 {
  let [a, b] = joined(&first, &second);
  let retained = b;
  let value = *retained;
  dispose(first);
  return value;
}
function childLocal(input: &i32): [&i32, &i32] {
  return join([() => input, () => { let local = 2i32; return &local; }]);
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (441944, 50918, 12),
        graph: (GraphCompletion::SourceRejected, (224503, 21166, 10)),
        pre_equality_graph: (224187, 21032, 10),
        roots: &[
            Root {
                name: "joined",
                want: Want::Checked,
                graph: None,
                stats: (26147, 15547, 7),
            },
            Root {
                name: "released",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"dispose(first)"####,
                    related: r####"&first"####,
                },
                graph: None,
                stats: (3321, 548, 6),
            },
            Root {
                name: "afterUse",
                want: Want::Checked,
                graph: None,
                stats: (32594, 16627, 7),
            },
            Root {
                name: "childLocal",
                want: Want::Rejected {
                    code: "R0202",
                    site: r####"return &local;"####,
                    related: r####"local"####,
                },
                graph: None,
                stats: (2718, 315, 10),
            },
        ],
    },
}];

#[test]
fn o01_shifted_point_waits_on_exclusive_deref_storage() {
    run(
        "rfc_cases::o01_shifted_point_waits_on_exclusive_deref_storage",
        O1,
    );
}

#[test]
fn o02_remembered_slot_waits_on_descriptor_slot_storage() {
    run(
        "rfc_cases::o02_remembered_slot_waits_on_descriptor_slot_storage",
        O2,
    );
}

#[test]
fn o03_identity_forwards_a_reference_but_its_roots_stay_incomplete() {
    run(
        "rfc_cases::o03_identity_forwards_a_reference_but_its_roots_stay_incomplete",
        O3,
    );
}

#[test]
fn o04_invalid_move_is_r0200() {
    run("rfc_cases::o04_invalid_move_is_r0200", O4);
}

#[test]
fn o05_disjoint_field_loans_check_and_field_arithmetic_blocks() {
    run(
        "rfc_cases::o05_disjoint_field_loans_check_and_field_arithmetic_blocks",
        O5,
    );
}

#[test]
fn o06_reborrowing_calls_wait_on_exclusive_deref_storage() {
    run(
        "rfc_cases::o06_reborrowing_calls_wait_on_exclusive_deref_storage",
        O6,
    );
}

#[test]
fn o07_overlapping_reference_fields_are_not_yet_rejected() {
    run(
        "rfc_cases::o07_overlapping_reference_fields_are_not_yet_rejected",
        O7,
    );
}

#[test]
fn o08_poisoned_slot_waits_on_descriptor_slot_storage() {
    run(
        "rfc_cases::o08_poisoned_slot_waits_on_descriptor_slot_storage",
        O8,
    );
}

#[test]
fn o09_retained_list_source_contract_is_incomplete_at_its_calls() {
    run(
        "rfc_cases::o09_retained_list_source_contract_is_incomplete_at_its_calls",
        O9,
    );
}

#[test]
fn o10_owned_list_reference_extraction_has_no_receipt() {
    run(
        "rfc_cases::o10_owned_list_reference_extraction_has_no_receipt",
        O10,
    );
}

#[test]
fn o11_exclusive_return_view_contract_is_incomplete_at_its_call() {
    run(
        "rfc_cases::o11_exclusive_return_view_contract_is_incomplete_at_its_call",
        O11,
    );
}

#[test]
fn o12_borrowed_temporary_is_incomplete_past_its_producer() {
    run(
        "rfc_cases::o12_borrowed_temporary_is_incomplete_past_its_producer",
        O12,
    );
}

#[test]
fn o13_joined_retained_sources_reject_early_release() {
    run(
        "rfc_cases::o13_joined_retained_sources_reject_early_release",
        O13,
    );
}

#[test]
fn o14_replacement_reaches_the_old_disposer_and_a_stale_view_rejects() {
    run(
        "rfc_cases::o14_replacement_reaches_the_old_disposer_and_a_stale_view_rejects",
        O14,
    );
}

/// Metering rule 11, D2 (b) (ri-4lq): the reached disposer's written
/// `dispose(value.text)` is now a verified graph edge, so `replace` checks in
/// the graph too. That adds 854 work and no entries; depth stays 5. The O14
/// graph cap before ri-4lq, 192,578/5,361/5, now gives `ResourceLimit`. The
/// row above holds the new cap.
#[test]
fn o14_pre_field_edge_graph_cap_now_gives_resource_limit() {
    installed(
        "rfc_cases::o14_pre_field_edge_graph_cap_now_gives_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O14[0].text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap();
                let (old, _) = limits(statistics((192578, 5361, 5)));
                let limited = analyze_ownership(&receipt, old);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.roots().iter().all(|root| root.mir().is_none()));
            }
        },
    );
}

#[test]
fn o15_counter_body_and_its_expectation_check() {
    run("rfc_cases::o15_counter_body_and_its_expectation_check", O15);
}

/// Metering rule 11: A2 (ri-785.1) charges the reads of `first` and `second`
/// with `charge_key`, so the pre-A2 counter caps (2,504/329/6 targeted,
/// 187,485/4,297/6 graph) now give `ResourceLimit`. The row above holds the new
/// caps and the ledger section in `m5-acceptance.md` the reason.
#[test]
fn o15_pre_charge_counter_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o15_pre_charge_counter_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O15[0].text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                assert_eq!(typed.completion(), TypedCompletion::Checked);
                let module = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                    .unwrap();
                let receipt = typed.receipt().unwrap();
                let counter = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function
                            && d.name.as_deref() == Some("counter")
                    })
                    .unwrap();
                let (old, _) = limits(statistics((2504, 329, 6)));
                let limited = lower_body(&receipt, module, &counter.id, old).unwrap();
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none() && limited.findings().is_empty());
                let (old, _) = limits(statistics((187485, 4297, 6)));
                assert_eq!(
                    analyze_ownership(&receipt, old).completion(),
                    GraphCompletion::ResourceLimit
                );
            }
        },
    );
}

/// Metering rule 11 (ri-785.9): the i32 equality expression lowers the O3, O5
/// and O15 comparisons, so the caps at which each expectation was `Incomplete`
/// at its comparison now give `ResourceLimit`. O15's expectation checks; O3's
/// and O5's stop at the blocker of the body they call. The rows above hold the
/// new caps.
#[test]
fn equality_expectations_exceed_their_incomplete_caps() {
    installed(
        "rfc_cases::equality_expectations_exceed_their_incomplete_caps",
        |request| {
            for (source, old) in [
                (O3, (1876, 23, 1)),
                (O5, (1902, 36, 1)),
                (O15, (1893, 32, 1)),
            ] {
                fs::write(&request.entry, format!("{IMPORTS}\n{}", source[0].text)).unwrap();
                for test_mode in [false, true] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let graph = loader::load_installed(request).unwrap();
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
                        .find(|d| d.id.kind == DeclarationKind::Expectation)
                        .unwrap();
                    let (old, _) = limits(statistics(old));
                    let limited =
                        lower_root(&receipt, module, BodyOwner::Declaration(&expect.id), old)
                            .unwrap();
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

#[test]
fn o16_two_shared_jobs_check_through_the_join() {
    run("rfc_cases::o16_two_shared_jobs_check_through_the_join", O16);
}

#[test]
fn o17_exclusive_copy_and_repeated_once_reject() {
    run(
        "rfc_cases::o17_exclusive_copy_and_repeated_once_reject",
        O17,
    );
}

#[test]
fn o18_custom_disposer_rejects_field_extraction() {
    run(
        "rfc_cases::o18_custom_disposer_rejects_field_extraction",
        O18,
    );
}

#[test]
fn o19_cleanup_order_holder_is_incomplete_at_its_construction() {
    run(
        "rfc_cases::o19_cleanup_order_holder_is_incomplete_at_its_construction",
        O19,
    );
}

#[test]
fn o20_transitive_guard_cleanup_is_incomplete_at_interpolation() {
    run(
        "rfc_cases::o20_transitive_guard_cleanup_is_incomplete_at_interpolation",
        O20,
    );
}

#[test]
fn o21_guard_release_replacement_reports_r0500() {
    run(
        "rfc_cases::o21_guard_release_replacement_reports_r0500",
        O21,
    );
}

#[test]
fn o22_descriptor_disposal_has_no_family_and_owner_disposal_rejects() {
    run(
        "rfc_cases::o22_descriptor_disposal_has_no_family_and_owner_disposal_rejects",
        O22,
    );
}

#[test]
fn o23_generic_no_borrow_evidence_rejects_r0302() {
    run(
        "rfc_cases::o23_generic_no_borrow_evidence_rejects_r0302",
        O23,
    );
}

#[test]
fn o24_scoped_children_keep_caller_sources_and_native_has_no_source_form() {
    run(
        "rfc_cases::o24_scoped_children_keep_caller_sources_and_native_has_no_source_form",
        O24,
    );
}
