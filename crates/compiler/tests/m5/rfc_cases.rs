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
use compiler::analysis::ownership::{GraphCompletion, Place, analyze_ownership, lower_root};
use compiler::analysis::scalars::{BodyOwner, Site};
use compiler::analysis::typed::TypedCompletion;
use compiler::loader::DeclarationKind;

#[path = "rfc_cases/list_contents.rs"]
mod list_contents;
#[path = "rfc_cases/o07.rs"]
mod o07;
#[path = "rfc_cases/o07_forms.rs"]
mod o07_forms;
#[path = "rfc_cases/o12.rs"]
mod o12;
#[path = "rfc_cases/o20.rs"]
mod o20;
#[path = "rfc_cases/record_contents.rs"]
mod record_contents;

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
    run_with(test, sources, |_, _, _| {});
}

/// `run`, then `extra` on each source's graph in each load mode.
fn run_with(test: &str, sources: &[Source], extra: impl Fn(usize, &loader::Graph, &str)) {
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
                        crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
                    }
                }
                extra(index, &graph, &context);
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
        // Related: the required provider the implementation would replace.
        let related = typed
            .ownership_protection()
            .iter()
            .find(|p| {
                p.owner.id == overlap.owner.id
                    && p.family == overlap.family
                    && p.rule == overlap.rule
            })
            .and_then(|p| p.provider.as_ref())
            .map(|provider| {
                format!(
                    "{} {}",
                    provider.implementation.id.module.path,
                    provider.implementation.span.bytes().escape_ascii()
                )
            });
        found.push(format!(
            "overlap {} @ {} (related: {})",
            overlap.code(),
            overlap.implementation.span.bytes().escape_ascii(),
            related.as_deref().unwrap_or("none")
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
        crate::limit::assert_typed_limit_keeping_body_findings(&limited);
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
        crate::limit::assert_graph_limit(&analyze_ownership(&receipt, short));
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
            crate::limit::assert_body_limit(&limited);
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
    // The parent measured typed `Incomplete` at (325375, 47829, 12) on
    // `pending Storage @ *value += 1i32;`; `o01_old_typed_caps_give_resource_limit`
    // pins that cap. `pre_equality_graph` is the graph cap with the expectation
    // root's attributed work and entries removed (8913, 1139).
    outcome: Outcome::Receipt {
        typed_stats: (326220, 47997, 12),
        graph: (GraphCompletion::Incomplete, (190759, 5183, 6)),
        pre_equality_graph: (181846, 4044, 6),
        roots: &[
            Root {
                name: "increment",
                want: Want::Checked,
                graph: None,
                stats: (2182, 183, 3),
            },
            Root {
                name: "shifted",
                want: Want::Checked,
                graph: None,
                stats: (5806, 722, 6),
            },
            Root {
                name: "expect",
                want: Want::Checked,
                graph: None,
                stats: (8913, 1139, 6),
            },
        ],
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
    // The parent measured typed `Incomplete` at (325943, 47994, 12) on
    // `pending Storage @ *slot = source;` and `&mut view`;
    // `o02_pre_record_contents_typed_cap_now_gives_resource_limit` pins it.
    // `pre_equality_graph` is the graph cap with the expectation root's
    // attributed work and entries removed (8410, 961).
    outcome: Outcome::Receipt {
        typed_stats: (326933, 48161, 12),
        graph: (GraphCompletion::Incomplete, (190814, 5004, 6)),
        pre_equality_graph: (182404, 4043, 6),
        roots: &[
            Root {
                name: "remember",
                want: Want::Checked,
                graph: None,
                stats: (2466, 167, 3),
            },
            Root {
                name: "selected",
                want: Want::Checked,
                graph: None,
                stats: (6373, 855, 6),
            },
            Root {
                name: "expect",
                want: Want::Checked,
                graph: None,
                stats: (8410, 961, 6),
            },
        ],
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
        graph: (GraphCompletion::Incomplete, (190039, 5288, 5)),
        pre_equality_graph: (187506, 4084, 3),
        roots: &[
            Root {
                name: "separate",
                want: Want::Checked,
                graph: None,
                stats: (4314, 1121, 4),
            },
            Root {
                name: "expect",
                want: Want::Checked,
                graph: None,
                stats: (6338, 1221, 4),
            },
        ],
    },
}];

/// O5's prose beyond `separate` (ri-fmsk): loans of distinct fields coexist and
/// the whole record reads after their last use; a loan of the same field, or of
/// the whole record, live at an acquisition conflicts, R0201.
const O5_LOANS: &[Source] = &[Source {
    text: r#"
type Pair = { left: i32; right: i32 };
function onlyLeft(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  *left += 1i32;
  return pair.left + pair.right;
}
function otherRead(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let right = pair.right;
  *left += 1i32;
  return right;
}
function wholeReadAfter(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let right = &mut pair.right;
  *left += 1i32;
  *right += 2i32;
  let whole = pair;
  return whole.left + whole.right;
}
function readLive(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let sum = pair.left + pair.right;
  *left += 1i32;
  return sum;
}
function sameField(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let first = &mut pair.left;
  let second = &mut pair.left;
  *first += 1i32;
  *second += 1i32;
  return pair.right;
}
function wholeBeforeFirst(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let whole = &mut pair;
  let left = &mut pair.left;
  *left += 1i32;
  let kept = whole;
  return 1i32;
}
function wholeBeforeSecond(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  *left += 1i32;
  let whole = &mut pair;
  let right = &mut pair.right;
  *right += 2i32;
  let kept = whole;
  return 1i32;
}
function wholeBetween(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let whole = &mut pair;
  *left += 1i32;
  let kept = whole;
  return 1i32;
}
function sharedWholeBeforeFirst(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let whole = &pair;
  let left = &mut pair.left;
  *left += 1i32;
  let kept = whole;
  return 1i32;
}
function wholeReadWhileLive(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let whole = pair;
  *left += 1i32;
  return 1i32;
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (347333, 51302, 12),
        graph: (GraphCompletion::SourceRejected, (214245, 9734, 5)),
        pre_equality_graph: (205449, 5349, 5),
        roots: &[
            Root {
                name: "onlyLeft",
                want: Want::Checked,
                graph: None,
                stats: (3802, 869, 4),
            },
            Root {
                name: "otherRead",
                want: Want::Checked,
                graph: None,
                stats: (3562, 749, 4),
            },
            Root {
                name: "wholeReadAfter",
                want: Want::Checked,
                graph: None,
                stats: (5493, 1596, 4),
            },
            Root {
                name: "readLive",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"pair.left"####,
                    related: r####"&mut pair.left"####,
                },
                graph: None,
                stats: (3248, 598, 4),
            },
            Root {
                name: "sameField",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&mut pair.left"####,
                    related: r####"&mut pair.left"####,
                },
                graph: None,
                stats: (3333, 636, 4),
            },
            Root {
                name: "wholeBeforeFirst",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&mut pair.left"####,
                    related: r####"&mut pair"####,
                },
                graph: None,
                stats: (3176, 561, 4),
            },
            Root {
                name: "wholeBeforeSecond",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&mut pair.right"####,
                    related: r####"&mut pair"####,
                },
                graph: None,
                stats: (3849, 900, 4),
            },
            Root {
                name: "wholeBetween",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&mut pair"####,
                    related: r####"&mut pair.left"####,
                },
                graph: None,
                stats: (3182, 563, 4),
            },
            Root {
                name: "sharedWholeBeforeFirst",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&mut pair.left"####,
                    related: r####"&pair"####,
                },
                graph: None,
                stats: (3177, 561, 4),
            },
            Root {
                name: "wholeReadWhileLive",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"pair"####,
                    related: r####"&mut pair.left"####,
                },
                graph: None,
                stats: (3505, 615, 4),
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
    // The parent measured typed `Incomplete` at (325245, 47754, 12) on
    // `pending Storage @ *value += 1i32;`; `o06_old_typed_caps_give_resource_limit`
    // pins that cap. `pre_equality_graph` is the graph cap with the expectation
    // root's attributed work and entries removed (8642, 688).
    outcome: Outcome::Receipt {
        typed_stats: (326089, 47923, 12),
        graph: (GraphCompletion::Incomplete, (189091, 4699, 6)),
        pre_equality_graph: (180449, 4011, 6),
        roots: &[
            Root {
                name: "change",
                want: Want::Checked,
                graph: None,
                stats: (2190, 187, 3),
            },
            Root {
                name: "twice",
                want: Want::Checked,
                graph: None,
                stats: (4412, 394, 3),
            },
            Root {
                name: "result",
                want: Want::Checked,
                graph: None,
                stats: (6615, 587, 6),
            },
            Root {
                name: "expect",
                want: Want::Checked,
                graph: None,
                stats: (8642, 688, 6),
            },
        ],
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
        graph: (GraphCompletion::SourceRejected, (187450, 4378, 5)),
        pre_equality_graph: (186712, 4049, 3),
        roots: &[Root {
            name: "overlap",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&value"####,
                related: r####"&mut value"####,
            },
            graph: None,
            stats: (2653, 282, 5),
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
    // The parent measured typed `Incomplete` at (327681, 48227, 12) on three
    // `pending Storage` signals; `o08_pre_record_contents_typed_cap_now_gives_resource_limit`
    // pins it. `pre_equality_graph` removes the expectation root (8494, 1003).
    outcome: Outcome::Receipt {
        typed_stats: (328745, 48394, 12),
        graph: (GraphCompletion::SourceRejected, (192713, 5271, 6)),
        pre_equality_graph: (184219, 4268, 6),
        roots: &[
            Root {
                name: "remember",
                want: Want::Checked,
                graph: None,
                stats: (2494, 181, 3),
            },
            Root {
                name: "selected",
                want: Want::Checked,
                graph: None,
                stats: (6429, 883, 6),
            },
            Root {
                name: "expect",
                want: Want::Checked,
                graph: None,
                stats: (8494, 1003, 6),
            },
            Root {
                name: "poison",
                want: Want::Rejected {
                    code: "R0202",
                    site: "*slot = &local;",
                    related: "&local",
                },
                graph: None,
                stats: (2815, 302, 4),
            },
        ],
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
    // The parent measured both roots `Incomplete` at their calls: `forwarded`
    // 1,930 / 55 / 3, `local` 2,292 / 118 / 3, whole graph 189,463 / 4,237 / 5
    // (typed unchanged at 329,382 / 48,563 / 12).
    // `o09_pre_list_contents_caps_now_give_resource_limit` pins them.
    outcome: Outcome::Receipt {
        typed_stats: (329382, 48563, 12),
        graph: (GraphCompletion::SourceRejected, (192306, 4641, 6)),
        pre_equality_graph: (189147, 4103, 3),
        roots: &[
            Root {
                name: "forwarded",
                want: Want::Checked,
                graph: None,
                stats: (3330, 243, 5),
            },
            Root {
                name: "local",
                want: Want::Rejected {
                    code: "R0202",
                    site: "retain(dst, &user)",
                    related: "&user",
                },
                graph: None,
                stats: (3726, 332, 6),
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
        outcome: Outcome::Receipt {
            typed_stats: (324131, 47845, 12),
            graph: (GraphCompletion::Incomplete, (188037, 4612, 6)),
            pre_equality_graph: (187721, 4478, 3),
            roots: &[Root {
                name: "extract",
                want: Want::Checked,
                graph: None,
                stats: (3539, 513, 6),
            }],
        },
    },
    Source {
        text: r#"
import std::collection::get;
type User = { id: i32 };
function slot(users: List<&User>): Option<&&User> { return get(&users, 0u64); }
"#,
        outcome: Outcome::Receipt {
            typed_stats: (325899, 48063, 12),
            graph: (GraphCompletion::SourceRejected, (187315, 4363, 5)),
            pre_equality_graph: (186999, 4229, 5),
            roots: &[Root {
                name: "slot",
                want: Want::Rejected {
                    code: "R0202",
                    site: "return get(&users, 0u64);",
                    related: "get(&users, 0u64)",
                },
                graph: None,
                stats: (3059, 267, 5),
            }],
        },
    },
    Source {
        text: r#"
import std::collection::get;
type User = { id: i32 };
function indexed(users: List<&User>, index: u64): Option<&&User> { return get(&users, index); }
"#,
        outcome: Outcome::Receipt {
            typed_stats: (325821, 48013, 12),
            graph: (GraphCompletion::SourceRejected, (187398, 4374, 5)),
            pre_equality_graph: (187082, 4240, 5),
            roots: &[Root {
                name: "indexed",
                want: Want::Rejected {
                    code: "R0202",
                    site: "return get(&users, index);",
                    related: "get(&users, index)",
                },
                graph: None,
                stats: (3081, 278, 5),
            }],
        },
    },
];

/// Rules 1 and 2: unsupported siblings of the extraction keep their parent caps.
const O10_SIBLINGS: &[Source] = &[
    Source {
        text: r#"
type User = { id: i32 };
function exclusive(users: List<&mut User>): Option<&mut User> {
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
function scalar(users: List<&i32>): Option<&i32> {
  for (let number of users) { return Option.Some(number); }
  return Option.None;
}
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"pending Storage @ for (let number of users) { return Option.Some(number); }"####,
            ],
            stats: (322461, 47631, 12),
        },
    },
];

/// Rules 1 and 2 (ri-2s0.7.2): lookups the ownership route does not admit keep
/// their parent caps. Each types `Checked` and stays `Incomplete` in ownership.
const O10_LOOKUP_SIBLINGS: &[Source] = &[Source {
    text: r#"
import std::collection::{get, first};
type User = { id: i32 };
function computed(users: List<&User>, index: u64): Option<&&User> { return get(&users, index + 1u64); }
function grouped(users: List<&User>): Option<&&User> { return get(&users, (0u64)); }
function view(users: &List<&User>): Option<&&User> { return get(users, 0u64); }
function head(users: List<&User>): Option<&&User> { return first(&users); }
function bound(users: List<&User>): Option<&&User> { let found = get(&users, 0u64); return found; }
function discarded(users: List<&User>): void { get(&users, 0u64); }
"#,
    outcome: Outcome::Receipt {
        typed_stats: (342897, 50941, 12),
        graph: (GraphCompletion::Incomplete, (191091, 4248, 5)),
        pre_equality_graph: (190775, 4114, 3),
        roots: &[
            Root {
                name: "computed",
                want: Want::Incomplete {
                    blocker: "get(&users, index + 1u64)",
                },
                graph: None,
                stats: (2009, 93, 2),
            },
            Root {
                name: "grouped",
                want: Want::Incomplete {
                    blocker: "get(&users, (0u64))",
                },
                graph: None,
                stats: (2003, 89, 2),
            },
            Root {
                name: "view",
                want: Want::Incomplete {
                    blocker: "get(users, 0u64)",
                },
                graph: None,
                stats: (1992, 89, 2),
            },
            Root {
                name: "head",
                want: Want::Incomplete {
                    blocker: "first(&users)",
                },
                graph: None,
                stats: (2003, 89, 2),
            },
            Root {
                name: "bound",
                want: Want::Incomplete { blocker: "found" },
                graph: None,
                stats: (2002, 89, 1),
            },
            Root {
                name: "discarded",
                want: Want::Incomplete {
                    blocker: "get(&users, 0u64)",
                },
                graph: None,
                stats: (2011, 89, 3),
            },
        ],
    },
}];

/// Rules 1, 2 and 5: `get` over a borrowed List of anything but shared
/// application-record references, and the grouped receiver, keep their parent
/// caps (the index literal keeps its hypothetical premise, as before ri-2s0.7.1).
const O10_GET_SIBLINGS: &[Source] = &[
    Source {
        text: r#"
import std::collection::get;
type User = { id: i32 };
function mutable(users: List<&mut User>): Option<&&mut User> { return get(&users, 0u64); }
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
    Source {
        text: r#"
import std::collection::get;
function scalar(users: List<&i32>): Option<&&i32> { return get(&users, 0u64); }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"obligation FamilySelection @ get(&users, 0u64)"####,
                r####"pending TypedInput @ get(&users, 0u64)"####,
                r####"pending TypedInput @ return get(&users, 0u64);"####,
            ],
            stats: (324494, 47829, 12),
        },
    },
    Source {
        text: r#"
import std::collection::get;
type User = { id: i32 };
function owned(users: List<User>): Option<&User> { return get(&users, 0u64); }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"obligation FamilySelection @ get(&users, 0u64)"####,
                r####"pending TypedInput @ get(&users, 0u64)"####,
                r####"pending TypedInput @ return get(&users, 0u64);"####,
            ],
            stats: (325250, 47838, 12),
        },
    },
    Source {
        text: r#"
import std::collection::get;
type User = { id: i32 };
function grouped(users: List<&User>): Option<&&User> { return get(&(users), 0u64); }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"obligation FamilySelection @ get(&(users), 0u64)"####,
                r####"pending TypedInput @ get(&(users), 0u64)"####,
                r####"pending TypedInput @ return get(&(users), 0u64);"####,
            ],
            stats: (325123, 47792, 12),
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
        graph: (GraphCompletion::SourceRejected, (191877, 4600, 6)),
        // The caps before ri-2s0.6 (metering rule 11, third case): the family
        // call now reaches its contract, so they give `ResourceLimit`.
        pre_equality_graph: (189575, 4219, 5),
        roots: &[
            Root {
                name: "conflict",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&owned"####,
                    related: r####"&mut owned"####,
                },
                graph: None,
                stats: (3028, 202, 6),
            },
            Root {
                name: "ordered",
                want: Want::Checked,
                graph: None,
                stats: (3316, 336, 6),
            },
        ],
    },
}];

/// Family-selected call siblings that stay unsupported (ri-2s0.6, rule 2).
const O11_SIBLINGS: &[Source] = &[
    Source {
        text: r#"
function editAndCount<L>(list: L): i32;
implement editAndCount(list: &mut List<i32>): i32 { while (true) {} }
function count(input: List<i32>): i32 {
  let mut owned = input;
  let total = editAndCount(&mut owned);
  return total;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (325674, 47936, 12),
            graph: (GraphCompletion::Incomplete, (187802, 4148, 5)),
            pre_equality_graph: (187486, 4014, 4),
            roots: &[Root {
                name: "count",
                want: Want::Incomplete {
                    blocker: r####"editAndCount(&mut owned)"####,
                },
                graph: None,
                stats: (1947, 47, 3),
            }],
        },
    },
    Source {
        text: r#"
function readView<L>(list: L): &i32;
implement readView(list: &List<i32>): &i32 { while (true) {} }
function shared(input: List<i32>): i32 {
  let owned = input;
  let view = readView(&owned);
  return *view;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (325841, 47984, 12),
            graph: (GraphCompletion::Incomplete, (187988, 4151, 5)),
            pre_equality_graph: (187672, 4017, 4),
            roots: &[Root {
                name: "shared",
                want: Want::Incomplete {
                    blocker: r####"readView(&owned)"####,
                },
                graph: None,
                stats: (1953, 50, 3),
            }],
        },
    },
    Source {
        text: r#"
function editAndView<L>(list: L): &i32;
implement editAndView(list: &mut List<i32>): &i32 { while (true) {} }
function grouped(input: List<i32>): i32 {
  let mut owned = input;
  let view = editAndView((&mut owned));
  return *view;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (326374, 48056, 12),
            graph: (GraphCompletion::Incomplete, (188051, 4152, 5)),
            pre_equality_graph: (187735, 4018, 4),
            roots: &[Root {
                name: "grouped",
                want: Want::Incomplete {
                    blocker: r####"editAndView((&mut owned))"####,
                },
                graph: None,
                stats: (1956, 51, 3),
            }],
        },
    },
    Source {
        text: r#"
function editAndView<L>(list: L): &i32;
implement editAndView(list: &mut List<i32>): &i32 { while (true) {} }
function looped(input: List<i32>): i32 {
  let mut owned = input;
  while (true) {
    let view = editAndView(&mut owned);
    return *view;
  }
  return 0i32;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (325933, 48025, 12),
            graph: (GraphCompletion::Incomplete, (188312, 4167, 5)),
            pre_equality_graph: (187996, 4033, 4),
            roots: &[Root {
                name: "looped",
                want: Want::Incomplete {
                    blocker: r####"editAndView(&mut owned)"####,
                },
                graph: None,
                stats: (1977, 66, 5),
            }],
        },
    },
];

/// Whole-List uses before the exclusive view's last use (ri-2s0.6).
const O11_NEGATIVES: &[Source] = &[Source {
    text: r#"
function editAndView<L>(list: L): &i32;
implement editAndView(list: &mut List<i32>): &i32 { while (true) {} }
function indexed(input: List<i32>): i32 {
  let mut owned = input;
  let view = editAndView(&mut owned);
  let first = (&owned)[0u64];
  return *view;
}
function reborrowed(input: List<i32>): i32 {
  let mut owned = input;
  let view = editAndView(&mut owned);
  let again = &mut owned;
  return *view;
}
function disposed(input: List<i32>): i32 {
  let mut owned = input;
  let view = editAndView(&mut owned);
  dispose(owned);
  return *view;
}
function moved(input: List<i32>): i32 {
  let mut owned = input;
  let view = editAndView(&mut owned);
  let other = owned;
  return *view;
}
function escaped(input: List<i32>): &i32 {
  let mut owned = input;
  let view = editAndView(&mut owned);
  return view;
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (338324, 49744, 12),
        graph: (GraphCompletion::SourceRejected, (198303, 5020, 6)),
        pre_equality_graph: (189575, 4219, 5),
        roots: &[
            Root {
                name: "indexed",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&owned"####,
                    related: r####"&mut owned"####,
                },
                graph: None,
                stats: (3174, 275, 6),
            },
            Root {
                name: "reborrowed",
                want: Want::Rejected {
                    code: "R0201",
                    site: r####"&mut owned"####,
                    related: r####"&mut owned"####,
                },
                graph: None,
                stats: (3119, 248, 6),
            },
            Root {
                name: "disposed",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"dispose(owned)"####,
                    related: r####"&mut owned"####,
                },
                graph: None,
                stats: (3158, 258, 6),
            },
            Root {
                name: "moved",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"owned"####,
                    related: r####"&mut owned"####,
                },
                graph: None,
                stats: (3137, 248, 6),
            },
            Root {
                name: "escaped",
                want: Want::Rejected {
                    code: "R0202",
                    site: r####"return view;"####,
                    related: r####"owned"####,
                },
                graph: None,
                stats: (3091, 237, 6),
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
        graph: (GraphCompletion::SourceRejected, (191182, 5092, 9)),
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
                want: Want::Checked,
                graph: None,
                stats: (5914, 661, 9),
            },
            Root {
                name: "stored",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"let view = &shifted(Point { x: 1, y: 2 });"####,
                    related: r####"&shifted(Point { x: 1, y: 2 })"####,
                },
                graph: None,
                stats: (2989, 361, 7),
            },
            Root {
                name: "literal",
                want: Want::Checked,
                graph: None,
                stats: (1949, 66, 4),
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
            graph: (GraphCompletion::SourceRejected, (193195, 5097, 8)),
            pre_equality_graph: (189687, 4102, 3),
            roots: &[
                Root {
                    name: "clear",
                    want: Want::Checked,
                    graph: None,
                    stats: (2047, 114, 1),
                },
                Root {
                    name: "joined",
                    want: Want::Rejected {
                        code: "R0203",
                        site: r####"dispose(left)"####,
                        related: r####"&left"####,
                    },
                    graph: None,
                    stats: (5150, 931, 8),
                },
            ],
        },
    },
];

/// Retained-store A (ri-2s0.11), stage 1: the same two sources through plain
/// functions, so the `&string` contents contract and the one-destination `clear`
/// call are checked without family admission. Written for this ticket.
const O13_PLAIN: &[Source] = &[Source {
    text: r#"
function remember(slot: &mut &string, source: &string): void { }
function clear(slot: &mut &string): void { }
function joined(flag: bool, left: string, right: string, seed: &string): void {
  let mut slot = seed;
  if (flag) { remember(&mut slot, &left); } else { remember(&mut slot, &right); }
  clear(&mut slot);
  dispose(left);
  let last = slot;
}
function loops(flag: bool, left: string, seed: &string): void {
  let mut slot = seed;
  while (flag) { remember(&mut slot, &left); continue; }
  clear(&mut slot);
  dispose(left);
  let last = slot;
}
function released(flag: bool, left: string, right: string, seed: &string): void {
  let mut slot = seed;
  if (flag) { remember(&mut slot, &left); } else { remember(&mut slot, &right); }
  clear(&mut slot);
  let last = slot;
  dispose(left);
  dispose(right);
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (337065, 49293, 12),
        graph: (GraphCompletion::SourceRejected, (203549, 7569, 8)),
        pre_equality_graph: (203548, 7569, 8),
        roots: &[
            Root {
                name: "remember",
                want: Want::Checked,
                graph: None,
                stats: (2234, 202, 1),
            },
            Root {
                name: "clear",
                want: Want::Checked,
                graph: None,
                stats: (2183, 182, 1),
            },
            Root {
                name: "joined",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"dispose(left)"####,
                    related: r####"&left"####,
                },
                graph: None,
                stats: (3746, 999, 8),
            },
            Root {
                name: "loops",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"dispose(left)"####,
                    related: r####"&left"####,
                },
                graph: None,
                stats: (3791, 1018, 8),
            },
            Root {
                name: "released",
                want: Want::Checked,
                graph: None,
                stats: (9199, 1887, 8),
            },
        ],
    },
}];

/// Retained-store A (ri-2s0.11), stage 2: the O13 family contract in a loop,
/// and with the release after the slot's last use. Written for this ticket.
const O13_FAMILY: &[Source] = &[Source {
    text: r#"
function remember<D, S>(slot: D, source: S): void;
implement remember(slot: &mut &string, source: &string): void { while (true) {} }
function clear(slot: &mut &string): void { }
function loops(flag: bool, left: string, seed: &string): void {
  let mut slot = seed;
  while (flag) { remember(&mut slot, &left); continue; }
  clear(&mut slot);
  dispose(left);
  let last = slot;
}
function released(flag: bool, left: string, right: string, seed: &string): void {
  let mut slot = seed;
  if (flag) { remember(&mut slot, &left); } else { remember(&mut slot, &right); }
  clear(&mut slot);
  let last = slot;
  dispose(left);
  dispose(right);
}
"#,
    outcome: Outcome::Receipt {
        typed_stats: (337785, 49820, 12),
        graph: (GraphCompletion::SourceRejected, (202579, 6570, 8)),
        pre_equality_graph: (202578, 6570, 8),
        roots: &[
            Root {
                name: "loops",
                want: Want::Rejected {
                    code: "R0203",
                    site: r####"dispose(left)"####,
                    related: r####"&left"####,
                },
                graph: None,
                stats: (4491, 983, 8),
            },
            Root {
                name: "released",
                want: Want::Checked,
                graph: None,
                stats: (8406, 1630, 8),
            },
        ],
    },
}];

/// Rule 2 (ri-2s0.11): a family store outside the admitted retained contract
/// keeps its parent blocker at the parent caps. A Bool beside a string
/// destination, three sources and a grouped destination are each refused by the
/// unmetered screen, so the call is still the blocker at the old cost. The caps
/// were measured at `1fb84233`, before the family-store admission.
const O13_SIBLINGS: &[Source] = &[
    Source {
        text: r#"
function remember<D, S, G>(slot: D, source: S, gate: G): void;
implement remember(slot: &mut &string, source: &string, gate: bool): void { while (true) {} }
function gated(flag: bool, left: string, seed: &string): void {
  let mut slot = seed;
  remember(&mut slot, &left, flag);
  dispose(left);
  let last = slot;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (328486, 48489, 12),
            graph: (GraphCompletion::Incomplete, (188672, 4174, 5)),
            pre_equality_graph: (188671, 4174, 5),
            roots: &[Root {
                name: "gated",
                want: Want::Incomplete {
                    blocker: r####"remember(&mut slot, &left, flag)"####,
                },
                graph: None,
                stats: (1969, 73, 3),
            }],
        },
    },
    Source {
        text: r#"
function remember<D, A, B, C>(slot: D, first: A, second: B, third: C): void;
implement remember(slot: &mut &string, first: &string, second: &string, third: &string): void { while (true) {} }
function triple(left: string, middle: string, right: string, seed: &string): void {
  let mut slot = seed;
  remember(&mut slot, &left, &middle, &right);
  dispose(left);
  let last = slot;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (331252, 49199, 12),
            graph: (GraphCompletion::Incomplete, (189179, 4186, 5)),
            pre_equality_graph: (189178, 4186, 5),
            roots: &[Root {
                name: "triple",
                want: Want::Incomplete {
                    blocker: r####"remember(&mut slot, &left, &middle, &right)"####,
                },
                graph: None,
                stats: (1992, 85, 3),
            }],
        },
    },
    Source {
        text: r#"
function remember<D, S>(slot: D, source: S): void;
implement remember(slot: &mut &string, source: &string): void { while (true) {} }
function grouped(left: string, seed: &string): void {
  let mut slot = seed;
  remember((&mut slot), &left);
  dispose(left);
  let last = slot;
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (328296, 48419, 12),
            graph: (GraphCompletion::Incomplete, (188476, 4167, 5)),
            pre_equality_graph: (188475, 4167, 5),
            roots: &[Root {
                name: "grouped",
                want: Want::Incomplete {
                    blocker: r####"remember((&mut slot), &left)"####,
                },
                graph: None,
                stats: (1957, 66, 3),
            }],
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
        graph: (GraphCompletion::Incomplete, (226867, 25858, 10)),
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
                want: Want::Checked,
                graph: None,
                stats: (36060, 21717, 10),
            },
            Root {
                name: "expect",
                want: Want::Checked,
                graph: None,
                stats: (38099, 21797, 10),
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
        graph: (GraphCompletion::Incomplete, (189320, 4794, 5)),
        pre_equality_graph: (186874, 4059, 3),
        roots: &[Root {
            name: "ordered",
            want: Want::Checked,
            graph: None,
            stats: (6121, 711, 5),
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
        graph: (GraphCompletion::Incomplete, (186732, 4311, 5)),
        pre_equality_graph: (186048, 4042, 3),
        roots: &[Root {
            name: "render",
            want: Want::Checked,
            graph: None,
            stats: (2499, 215, 4),
        }],
    },
}];

const O21: &[Source] = &[
    Source {
        text: r#"
import std::core::dispose as release;
import std::mutex::LockGuard;
type Payload = { id: i32; }
implement release(value: LockGuard<Payload>): void { }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::SourceRejected,
            signals: &[
                r####"overlap R0500 @ implement release(value: LockGuard<Payload>): void { } (related: mutex.rill implement<T> dispose(value: LockGuard<T>): void { releaseGuard(value); })"####,
            ],
            stats: (324267, 47658, 12),
        },
    },
    // Siblings with a second obligation: since ri-2s0.9 both are `SourceRejected`
    // (R0500 only); the replacement's own body and an unrelated root of the same
    // module both call `release(view)` on a `&string`, which O22 admits. The
    // `Incomplete` witness is the last source.
    Source {
        text: r#"
import std::core::dispose as release;
import std::mutex::LockGuard;
type Payload = { id: i32; }
implement release(value: LockGuard<Payload>): void { let text = "a"; let view = &text; release(view); }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::SourceRejected,
            signals: &[
                r####"overlap R0500 @ implement release(value: LockGuard<Payload>): void { let text = \"a\"; let view = &text; release(view); } (related: mutex.rill implement<T> dispose(value: LockGuard<T>): void { releaseGuard(value); })"####,
            ],
            stats: (324760, 47779, 12),
        },
    },
    Source {
        text: r#"
import std::core::dispose as release;
import std::mutex::LockGuard;
type Payload = { id: i32; }
implement release(value: LockGuard<Payload>): void { }
function ended(owner: string): void {
  let view = &owner;
  release(view);
}
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::SourceRejected,
            signals: &[
                r####"overlap R0500 @ implement release(value: LockGuard<Payload>): void { } (related: mutex.rill implement<T> dispose(value: LockGuard<T>): void { releaseGuard(value); })"####,
            ],
            stats: (326030, 47803, 12),
        },
    },
    // Witness that a second obligation still keeps the module `Incomplete` after
    // O22 (ri-2s0.9, chief ruling): `release(view)` of a `&mut string` is not a
    // shared descriptor end, so its `FamilySelection` obligation stays unresolved.
    Source {
        text: r#"
import std::core::dispose as release;
import std::mutex::LockGuard;
type Payload = { id: i32; }
implement release(value: LockGuard<Payload>): void { let mut text = "a"; let view = &mut text; release(view); }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"obligation FamilySelection @ release(view)"####,
                r####"pending TypedInput @ release(view)"####,
                r####"overlap R0500 @ implement release(value: LockGuard<Payload>): void { let mut text = \"a\"; let view = &mut text; release(view); } (related: mutex.rill implement<T> dispose(value: LockGuard<T>): void { releaseGuard(value); })"####,
            ],
            stats: (324633, 47683, 12),
        },
    },
];

/// O21's exact typed cap before the replacement was rejected at collection
/// (ri-8qlm, metering rule 11): the old `Incomplete` result stopped at the
/// body's `FamilySelection` obligation. It now gives `ResourceLimit`.
const O21_PRE_REJECTION: Triple = (323823, 47493, 12);

/// Exact typed caps of O21's two siblings measured at 7002855e, before the
/// rejection (metering rules 1 and 2). Each completed `Incomplete` there at the
/// `dispose(view)` `FamilySelection` obligation. Since ri-2s0.9 that call ends
/// its descriptor, each sibling is `SourceRejected` (R0500 only) and these
/// caps give `ResourceLimit`.
const O21_SIBLINGS_PARENT: [Triple; 2] = [(325035, 47684, 12), (326297, 47705, 12)];

/// O22 source 0's exact typed cap before `dispose(view)` of a `&string` ended
/// that descriptor (ri-2s0.9, metering rule 11): the old `Incomplete` result
/// stopped at each `dispose(view)` `FamilySelection` obligation. The new
/// `Checked` result needs 64 more entries, so the old cap gives `ResourceLimit`.
const O22_PRE_DESCRIPTOR_END: Triple = (325919, 47848, 12);

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
        outcome: Outcome::Receipt {
            typed_stats: (325422, 47912, 12),
            graph: (GraphCompletion::SourceRejected, (188285, 4569, 5)),
            pre_equality_graph: (187969, 4435, 5),
            roots: &[
                Root {
                    name: "aliased",
                    want: Want::Rejected {
                        code: "R0203",
                        site: r####"dispose(owner)"####,
                        related: r####"&owner"####,
                    },
                    graph: None,
                    stats: (2303, 237, 4),
                },
                Root {
                    name: "ended",
                    want: Want::Checked,
                    graph: None,
                    stats: (2383, 264, 4),
                },
            ],
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
    Source {
        text: r#"
function reused(owner: string): void {
  let view = &owner;
  dispose(view);
  let again = view;
}
function released(owner: string): void {
  let view = &owner;
  dispose(view);
  dispose(owner);
}
"#,
        outcome: Outcome::Receipt {
            typed_stats: (325124, 47814, 12),
            graph: (GraphCompletion::SourceRejected, (188456, 4447, 5)),
            pre_equality_graph: (188140, 4313, 5),
            roots: &[
                Root {
                    name: "released",
                    want: Want::Checked,
                    graph: None,
                    stats: (2244, 217, 4),
                },
                Root {
                    name: "reused",
                    want: Want::Rejected {
                        code: "R0200",
                        site: r####"view"####,
                        related: r####"dispose(view)"####,
                    },
                    graph: None,
                    stats: (2116, 155, 4),
                },
            ],
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

/// Writes through an exclusive deref that no admission covers: a non-`i32`
/// target, a shared `&i32`, a plain store, `-=`, `*=` and a record field. Their
/// typed caps are the parent's (7002855e), measured in a `git worktree add`.
const EXCLUSIVE_WRITES_AT_PARENT: &[Source] = &[
    Source {
        text: r#"
function bump(value: &mut i64): void { *value += 1i64; }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[r####"pending Storage @ *value += 1i64;"####],
            stats: (321407, 47379, 12),
        },
    },
    Source {
        text: r#"
function bump(value: &i32): void { *value += 1i32; }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"finding ReadOnly @ *value += 1i32;"####,
                r####"pending Storage @ *value += 1i32;"####,
            ],
            stats: (321795, 47374, 12),
        },
    },
    Source {
        text: r#"
function bump(value: &mut i32): void { *value = 1i32; }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[r####"pending Storage @ *value = 1i32;"####],
            stats: (321406, 47379, 12),
        },
    },
    Source {
        text: r#"
function bump(value: &mut i32): void { *value -= 1i32; }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[r####"pending Storage @ *value -= 1i32;"####],
            stats: (321405, 47379, 12),
        },
    },
    Source {
        text: r#"
function bump(value: &mut i32): void { *value *= 2i32; }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[r####"pending Storage @ *value *= 2i32;"####],
            stats: (321405, 47379, 12),
        },
    },
    Source {
        text: r#"
type Pair = { x: i32; y: i32 };
function bump(value: &mut Pair): void { value.x += 1i32; }
"#,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[r####"pending Storage @ value.x += 1i32;"####],
            stats: (322826, 47449, 12),
        },
    },
];

/// Siblings of the admitted `*parameter += literal`. Each `Incomplete` root
/// keeps the caps measured with only the typed admission applied, in a
/// `git worktree add` at the parent, before the ownership lowering existed:
/// the lowering screens from written shape and paid-for facts before any charge.
/// The supported wrapped forms (`scoped`, `looped`, `dead`) are Checked controls.
const EXCLUSIVE_WRITE_SIBLINGS: &[Source] = &[Source {
    text: r#"
function control(value: &mut i32): void { *value += 1i32; }
function groupedTarget(value: &mut i32): void { (*value) += 1i32; }
function groupedOperand(value: &mut i32): void { *value += (1i32); }
function groupedDescriptor(value: &mut i32): void { *(value) += 1i32; }
function scoped(value: &mut i32): void { { *value += 1i32; } }
function looped(value: &mut i32, flag: bool): void { while (flag) { *value += 1i32; } }
function dead(value: &mut i32): void { return; *value += 1i32; }
function computed(value: &mut i32, step: i32): void { *value += step; }
function viaLocal(value: &mut i32): void { let view = value; *view += 1i32; }
"#,
    outcome: Outcome::Receipt {
        typed_stats: (335273, 48561, 12),
        graph: (GraphCompletion::Incomplete, (192826, 4858, 5)),
        pre_equality_graph: (190538, 4622, 5),
        roots: &[
            Root {
                name: "control",
                want: Want::Checked,
                graph: None,
                stats: (2288, 236, 3),
            },
            Root {
                name: "groupedTarget",
                want: Want::Incomplete {
                    blocker: r####"(*value) += 1i32;"####,
                },
                graph: None,
                stats: (1998, 92, 2),
            },
            Root {
                name: "groupedOperand",
                want: Want::Incomplete {
                    blocker: r####"*value += (1i32);"####,
                },
                graph: None,
                stats: (1998, 92, 2),
            },
            Root {
                name: "groupedDescriptor",
                want: Want::Incomplete {
                    blocker: r####"*(value) += 1i32;"####,
                },
                graph: None,
                stats: (1998, 92, 2),
            },
            Root {
                name: "scoped",
                want: Want::Checked,
                graph: None,
                stats: (2290, 236, 4),
            },
            Root {
                name: "looped",
                want: Want::Checked,
                graph: None,
                stats: (2510, 340, 5),
            },
            Root {
                name: "dead",
                want: Want::Checked,
                graph: None,
                stats: (2055, 118, 2),
            },
            Root {
                name: "computed",
                want: Want::Incomplete {
                    blocker: r####"*value += step;"####,
                },
                graph: None,
                stats: (2004, 96, 2),
            },
            Root {
                name: "viaLocal",
                want: Want::Incomplete {
                    blocker: r####"*view += 1i32;"####,
                },
                graph: None,
                stats: (2008, 101, 2),
            },
        ],
    },
}];

/// `call().field == i32` operands. `pre_equality_graph` is the whole graph
/// before the field-of-call operand was admitted (193367, 4510, 5), measured in a
/// `git worktree add` at the parent with only the typed admission. `bound` and
/// `nested` keep the parent's caps: a bound field read and a nested field are
/// not operands the equality admits.
const CALL_FIELD_EQUALITY: &[Source] = &[Source {
    text: r#"
type Point = { x: i32; y: i32 };
type Outer = { inner: Point };
function mk(): Point { return Point { x: 2i32, y: 3i32 }; }
function wrap(): Outer { return Outer { inner: mk() }; }
function bare(): bool { return mk().x == 2i32; }
function grouped(): bool { return ((mk()).x) == (2i32); }
function both(): bool { return mk().x == mk().y; }
function viaLocal(n: i32): bool { return n == mk().x; }
function bound(): i32 { let n = mk().x; return n; }
function nested(): bool { return wrap().inner.x == 2i32; }
function dead(): bool { return false; return mk().x == 2i32; }
function looped(flag: bool): bool { while (flag) { return mk().x == 2i32; } return false; }
"#,
    outcome: Outcome::Receipt {
        typed_stats: (340057, 48998, 12),
        graph: (GraphCompletion::Incomplete, (197917, 5950, 6)),
        pre_equality_graph: (193367, 4510, 5),
        roots: &[
            Root {
                name: "mk",
                want: Want::Checked,
                graph: None,
                stats: (2517, 231, 4),
            },
            Root {
                name: "wrap",
                want: Want::Incomplete {
                    blocker: r####"Outer { inner: mk() }"####,
                },
                graph: None,
                stats: (2366, 155, 2),
            },
            Root {
                name: "bare",
                want: Want::Checked,
                graph: None,
                stats: (5192, 547, 4),
            },
            Root {
                name: "grouped",
                want: Want::Checked,
                graph: None,
                stats: (5197, 547, 5),
            },
            Root {
                name: "both",
                want: Want::Checked,
                graph: None,
                stats: (5705, 687, 4),
            },
            Root {
                name: "viaLocal",
                want: Want::Checked,
                graph: None,
                stats: (5282, 593, 4),
            },
            Root {
                name: "bound",
                want: Want::Incomplete {
                    blocker: r####"mk().x"####,
                },
                graph: None,
                stats: (2038, 108, 3),
            },
            Root {
                name: "nested",
                want: Want::Incomplete {
                    blocker: r####"wrap().inner.x == 2i32"####,
                },
                graph: None,
                stats: (2017, 98, 3),
            },
            Root {
                name: "dead",
                want: Want::Checked,
                graph: None,
                stats: (2071, 127, 3),
            },
            Root {
                name: "looped",
                want: Want::Checked,
                graph: None,
                stats: (5493, 720, 6),
            },
        ],
    },
}];

/// The parent's typed cap for a source this ticket now admits reports
/// `ResourceLimit` with no receipt (metering rule 11, the ticket's mandated flip).
fn old_typed_caps(test: &str, sources: &[Source], old: Triple) {
    installed(test, |request| {
        for source in sources {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", source.text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let (parent, _) = limits(statistics(old));
                let typed = analyze_typed(&graph, parent);
                crate::limit::assert_typed_limit(&typed);
                assert!(typed.receipt().is_none(), "test_mode {test_mode}");
            }
        }
    });
}

#[test]
fn o01_shifted_point_checks_through_exclusive_deref_storage() {
    run(
        "rfc_cases::o01_shifted_point_checks_through_exclusive_deref_storage",
        O1,
    );
}

#[test]
fn o01_old_typed_caps_give_resource_limit() {
    old_typed_caps(
        "rfc_cases::o01_old_typed_caps_give_resource_limit",
        O1,
        (325375, 47829, 12),
    );
}

#[test]
fn o02_remembered_slot_carries_the_stored_source_to_the_caller() {
    run(
        "rfc_cases::o02_remembered_slot_carries_the_stored_source_to_the_caller",
        O2,
    );
}

#[test]
fn o02_pre_record_contents_typed_cap_now_gives_resource_limit() {
    old_typed_caps(
        "rfc_cases::o02_pre_record_contents_typed_cap_now_gives_resource_limit",
        O2,
        (325943, 47994, 12),
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
fn o05_disjoint_field_loans_check_through_local_field_descriptors() {
    run(
        "rfc_cases::o05_disjoint_field_loans_check_through_local_field_descriptors",
        O5,
    );
}

#[test]
fn o05_field_loan_conflicts_reject_and_disjoint_forms_check() {
    run(
        "rfc_cases::o05_field_loan_conflicts_reject_and_disjoint_forms_check",
        O5_LOANS,
    );
}

#[test]
fn o06_reborrowing_calls_check_through_exclusive_deref_storage() {
    run(
        "rfc_cases::o06_reborrowing_calls_check_through_exclusive_deref_storage",
        O6,
    );
}

#[test]
fn o06_old_typed_caps_give_resource_limit() {
    old_typed_caps(
        "rfc_cases::o06_old_typed_caps_give_resource_limit",
        O6,
        (325245, 47754, 12),
    );
}

#[test]
fn o01_unadmitted_exclusive_writes_keep_their_parent_caps() {
    run(
        "rfc_cases::o01_unadmitted_exclusive_writes_keep_their_parent_caps",
        EXCLUSIVE_WRITES_AT_PARENT,
    );
}

#[test]
fn o01_exclusive_i32_write_siblings_keep_their_screens() {
    run(
        "rfc_cases::o01_exclusive_i32_write_siblings_keep_their_screens",
        EXCLUSIVE_WRITE_SIBLINGS,
    );
}

#[test]
fn o01_call_field_equality_operands_check_and_siblings_keep_their_screens() {
    run(
        "rfc_cases::o01_call_field_equality_operands_check_and_siblings_keep_their_screens",
        CALL_FIELD_EQUALITY,
    );
}

#[test]
fn o07_overlapping_reference_fields_reject_r0201() {
    run(
        "rfc_cases::o07_overlapping_reference_fields_reject_r0201",
        O7,
    );
}

/// Metering rule 11 (ri-pfxi's mandated flip): admitting the `&mut` member
/// moved the O7 caps, so the caps before it, `overlap` 2,233/88/1 and the
/// graph 187,028/4,183/5 (both `Incomplete` at `holder`), now give
/// `ResourceLimit`. `m5-acceptance/ri-pfxi.md` holds the reason.
#[test]
fn o07_pre_admission_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o07_pre_admission_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O7[0].text)).unwrap();
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
                let overlap = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some("overlap"))
                    .unwrap();
                let (old, _) = limits(statistics((2233, 88, 1)));
                let limited = lower_body(&receipt, module, &overlap.id, old).unwrap();
                crate::limit::assert_body_limit(&limited);
                let (old, _) = limits(statistics((187028, 4183, 5)));
                crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
            }
        },
    );
}

#[test]
fn o08_poisoned_slot_rejects_the_escape_of_a_callee_local() {
    run(
        "rfc_cases::o08_poisoned_slot_rejects_the_escape_of_a_callee_local",
        O8,
    );
}

#[test]
fn o08_pre_record_contents_typed_cap_now_gives_resource_limit() {
    old_typed_caps(
        "rfc_cases::o08_pre_record_contents_typed_cap_now_gives_resource_limit",
        O8,
        (327681, 48227, 12),
    );
}

#[test]
fn o09_retained_list_source_checks_forwarded_and_rejects_local() {
    run(
        "rfc_cases::o09_retained_list_source_checks_forwarded_and_rejects_local",
        O9,
    );
}

/// Metering rule 11, mandated flips (ri-ema): both roots and the whole graph
/// moved, so each parent cap now gives `ResourceLimit`.
#[test]
fn o09_pre_list_contents_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o09_pre_list_contents_caps_now_give_resource_limit",
        |request| {
            list_contents::old_root_caps(
                &request,
                &[(
                    O9[0].text,
                    &[("forwarded", (1930, 55, 3)), ("local", (2292, 118, 3))],
                    (189463, 4237, 5),
                )],
            );
        },
    );
}

#[test]
fn o10_extracted_reference_checks_and_slot_reference_is_rejected() {
    run(
        "rfc_cases::o10_extracted_reference_checks_and_slot_reference_is_rejected",
        O10,
    );
}

#[test]
fn o10_extraction_siblings_keep_their_parent_caps() {
    run(
        "rfc_cases::o10_extraction_siblings_keep_their_parent_caps",
        O10_SIBLINGS,
    );
}

/// Metering rule 11: admitting the extraction moved its typed cap, so the
/// cap before ri-2s0.7, 323,289/47,677/12, now gives `ResourceLimit`.
#[test]
fn o10_pre_extraction_typed_cap_now_gives_resource_limit() {
    installed(
        "rfc_cases::o10_pre_extraction_typed_cap_now_gives_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O10[0].text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let (old, _) = limits(statistics((323289, 47677, 12)));
                let typed = analyze_typed(&graph, old);
                crate::limit::assert_typed_limit(&typed);
            }
        },
    );
}

#[test]
fn o10_lookup_siblings_keep_their_parent_caps() {
    run(
        "rfc_cases::o10_lookup_siblings_keep_their_parent_caps",
        O10_LOOKUP_SIBLINGS,
    );
}

#[test]
fn o10_get_siblings_keep_their_parent_caps() {
    run(
        "rfc_cases::o10_get_siblings_keep_their_parent_caps",
        O10_GET_SIBLINGS,
    );
}

/// Metering rule 11, mandated flip (ri-2s0.7.1): admitting `get(&users, 0u64)`
/// on a `List<&Record>` moved its typed cap, so the cap before it,
/// 325,319/47,875/12, now gives `ResourceLimit`.
#[test]
fn o10_pre_slot_get_typed_cap_now_gives_resource_limit() {
    installed(
        "rfc_cases::o10_pre_slot_get_typed_cap_now_gives_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O10[1].text)).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let (old, _) = limits(statistics((325319, 47875, 12)));
                let typed = analyze_typed(&graph, old);
                crate::limit::assert_typed_limit(&typed);
            }
        },
    );
}

/// Metering rule 11, mandated flip (ri-2s0.7.2): rejecting the reference to the
/// destroyed element slot moved the `slot` root and the whole graph, so the caps
/// before it (root 1,869/22/2, graph 186,123/4,117/5) now give `ResourceLimit`.
#[test]
fn o10_pre_slot_lookup_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o10_pre_slot_lookup_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O10[1].text)).unwrap();
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
                let root = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some("slot")
                    })
                    .unwrap();
                let (old, _) = limits(statistics((1869, 22, 2)));
                let limited = lower_body(&receipt, module, &root.id, old).unwrap();
                crate::limit::assert_body_limit(&limited);
                let (old, _) = limits(statistics((186123, 4117, 5)));
                crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
            }
        },
    );
}

#[test]
fn o11_exclusive_return_view_rejects_whole_read_before_last_use() {
    run(
        "rfc_cases::o11_exclusive_return_view_rejects_whole_read_before_last_use",
        O11,
    );
}

/// Metering rule 11, third case (ri-2s0.6): the family call used to stop at
/// `editAndView(&mut owned)`; it now reaches the implementation's declared
/// contract, so the old `Incomplete` caps give `ResourceLimit` per root and for
/// the whole graph. The O11 row holds the new caps.
#[test]
fn o11_pre_family_call_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o11_pre_family_call_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O11[0].text)).unwrap();
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
                for (name, old) in [("conflict", (2014, 77, 4)), ("ordered", (2032, 83, 4))] {
                    let root = module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.id.kind == DeclarationKind::Function
                                && d.name.as_deref() == Some(name)
                        })
                        .unwrap();
                    let (old, _) = limits(statistics(old));
                    let limited = lower_body(&receipt, module, &root.id, old).unwrap();
                    crate::limit::assert_body_limit(&limited);
                }
                let (old, _) = limits(statistics((189575, 4219, 5)));
                crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
            }
        },
    );
}

/// Rule 2: a family call outside the admitted O11 shape keeps its parent
/// blocker at the parent caps (the call is still the blocker).
#[test]
fn o11_unsupported_family_call_siblings_stay_incomplete_at_their_calls() {
    run(
        "rfc_cases::o11_unsupported_family_call_siblings_stay_incomplete_at_their_calls",
        O11_SIBLINGS,
    );
}

#[test]
fn o11_whole_list_uses_before_the_view_ends_do_not_check() {
    run(
        "rfc_cases::o11_whole_list_uses_before_the_view_ends_do_not_check",
        O11_NEGATIVES,
    );
}

#[test]
fn o12_borrowed_temporary_lives_through_its_statement_and_static_view_checks() {
    run(
        "rfc_cases::o12_borrowed_temporary_lives_through_its_statement_and_static_view_checks",
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
fn o13_plain_string_retained_contract_rejects_early_release() {
    run(
        "rfc_cases::o13_plain_string_retained_contract_rejects_early_release",
        O13_PLAIN,
    );
}

#[test]
fn o13_family_store_in_a_loop_rejects_and_after_the_last_use_checks() {
    run(
        "rfc_cases::o13_family_store_in_a_loop_rejects_and_after_the_last_use_checks",
        O13_FAMILY,
    );
}

/// Metering rule 11, third case (ri-2s0.11): the second O13 source's `clear`
/// root now holds retained `&string` contents, and `remember(&mut slot, ..)`
/// is read from the implementation's declared contract, so the caps at which
/// `joined` was `Incomplete` at `&mut slot` now give `ResourceLimit` per root
/// and for the whole graph. The O13 row holds the new caps.
#[test]
fn o13_pre_retained_string_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o13_pre_retained_string_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O13[1].text)).unwrap();
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
                for (name, old) in [("clear", (1993, 88, 1)), ("joined", (2015, 97, 3))] {
                    let root = module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.id.kind == DeclarationKind::Function
                                && d.name.as_deref() == Some(name)
                        })
                        .unwrap();
                    let (old, _) = limits(statistics(old));
                    let limited = lower_body(&receipt, module, &root.id, old).unwrap();
                    crate::limit::assert_body_limit(&limited);
                }
                let (old, _) = limits(statistics((190003, 4236, 5)));
                crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
            }
        },
    );
}

/// Rule 2: the siblings of the family store stay `Incomplete` at their calls,
/// at the caps the parent measured.
#[test]
fn o13_unsupported_family_store_siblings_stay_incomplete_at_their_calls() {
    run(
        "rfc_cases::o13_unsupported_family_store_siblings_stay_incomplete_at_their_calls",
        O13_SIBLINGS,
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
                crate::limit::assert_graph_limit(&limited);
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
                crate::limit::assert_body_limit(&limited);
                let (old, _) = limits(statistics((187485, 4297, 6)));
                let report = analyze_ownership(&receipt, old);
                crate::limit::assert_graph_limit(&report);
                // Rule 8 (review ci-uaa1 R-5): no MIR and no finding either.
                assert!(report.findings().is_empty());
                assert!(
                    report
                        .roots()
                        .iter()
                        .all(|root| root.mir().is_none() && root.ordinary_mir().is_none())
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
                    crate::limit::assert_body_limit(&limited);
                }
            }
        },
    );
}

/// Metering rule 11, third case (ri-fmsk): O5's `separate` and its expectation
/// now check, so the caps at which they were `Incomplete` at
/// `pair.left + pair.right`, and the graph cap with them, give `ResourceLimit`.
#[test]
fn o05_old_caps_give_resource_limit() {
    installed("rfc_cases::o05_old_caps_give_resource_limit", |request| {
        fs::write(&request.entry, format!("{IMPORTS}\n{}", O5[0].text)).unwrap();
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
            let (old, _) = limits(statistics((188006, 4283, 5)));
            let whole = analyze_ownership(&receipt, old);
            crate::limit::assert_graph_limit(&whole);
            for (kind, old) in [
                (DeclarationKind::Function, (2281, 116, 3)),
                (DeclarationKind::Expectation, (4305, 216, 3)),
            ] {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.id.kind == kind)
                    .unwrap();
                let (old, _) = limits(statistics(old));
                let limited = if kind == DeclarationKind::Expectation {
                    lower_root(
                        &receipt,
                        module,
                        BodyOwner::Declaration(&declaration.id),
                        old,
                    )
                } else {
                    lower_body(&receipt, module, &declaration.id, old)
                }
                .unwrap();
                crate::limit::assert_body_limit(&limited);
            }
        }
    });
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
fn o19_cleanup_order_disposes_the_holder_before_its_source() {
    run(
        "rfc_cases::o19_cleanup_order_disposes_the_holder_before_its_source",
        O19,
    );
}

/// A place as written, with its field when it has one. The compiler-only
/// epilogue that receives a disposer's remaining fields is `epilogue`.
fn place_text(place: &Place<'_>, epilogue: Option<usize>) -> String {
    let root = if Some(place.root.node) == epilogue {
        "epilogue".to_string()
    } else {
        String::from_utf8_lossy(place.root.span.bytes()).into_owned()
    };
    match place.field() {
        Some(field) => format!("{root}.{}", field.name.as_deref().unwrap()),
        None => root,
    }
}

/// The whole ordered action list of a straight path, one `(kind, place)` per
/// action, with no filtering.
fn o19_sequence(mir: &Mir<'_>) -> Vec<(&'static str, String)> {
    let blocks = straight_blocks(mir, mir.entry);
    assert!(matches!(
        blocks.last().unwrap().terminator,
        Terminator::Return
    ));
    let mut epilogue = None;
    let mut events = Vec::new();
    for block in blocks {
        let text = |place: &Place<'_>| place_text(place, epilogue);
        match &block.operation {
            Operation::None => {}
            Operation::Initialize(place) => events.push(("init", text(place))),
            Operation::Copy { from, into } => events.push((
                "copy",
                match into {
                    Some(into) => format!("{} -> {}", text(from), text(into)),
                    None => text(from),
                },
            )),
            Operation::Borrow {
                source,
                holder,
                exclusive: false,
            } => events.push(("borrow", format!("{} -> {}", text(source), text(holder)))),
            Operation::Install { target, incoming } => {
                events.push(("install", format!("{} -> {}", text(incoming), text(target))))
            }
            Operation::Cleanup(actions) => {
                for action in actions {
                    if let Obligation::DeferDisposerFields { into, .. } = action {
                        epilogue = Some(into.root.node);
                    }
                    let text = |place: &Place<'_>| place_text(place, epilogue);
                    events.push(match action {
                        Obligation::EndStorage(place) => ("end", text(place)),
                        Obligation::Drop(place) => ("drop", text(place)),
                        Obligation::InvokeDisposer { operand, .. } => ("disposer", text(operand)),
                        Obligation::DeferDisposerFields { from, into } => {
                            ("defer", format!("{} -> {}", text(from), text(into)))
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

/// RFC 0007 O19 (ri-kdi): the replaced holder's disposer runs before the new
/// value is installed, and at the end of the scope the holder's disposer,
/// which reads `view`, runs before `source` is dropped. Targeted query and
/// ordinary graph root, in both load modes.
#[test]
fn o19_cleanup_sequence_disposes_the_holder_before_drop_source() {
    installed(
        "rfc_cases::o19_cleanup_sequence_disposes_the_holder_before_drop_source",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O19[0].text)).unwrap();
            let expected: Vec<(&str, String)> = [
                ("copy", "seed: &string -> holder.view"),
                ("init", "holder"),
                ("init", "source"),
                ("borrow", "source -> Holder { view: &source }.view"),
                ("init", "Holder { view: &source }"),
                // The replaced value, before the new one is installed.
                ("disposer", "holder"),
                ("install", "Holder { view: &source } -> holder"),
                // The dependent holder before the later-declared source.
                ("disposer", "holder"),
                ("drop", "source"),
                ("end", "seed: &string"),
            ]
            .into_iter()
            .map(|(kind, place)| (kind, place.to_string()))
            .collect();
            // The active disposer reads `view` from its consumed operand,
            // then ends the remaining fields it did not move.
            let active: Vec<(&str, String)> = [
                ("copy", "value: Holder.view -> seen"),
                ("end", "seen"),
                ("defer", "value: Holder -> epilogue"),
                ("end", "epilogue.view"),
                ("end", "epilogue"),
            ]
            .into_iter()
            .map(|(kind, place)| (kind, place.to_string()))
            .collect();
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
                let find = |kind, name: Option<&str>| {
                    module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.id.kind == kind && (name.is_none() || d.name.as_deref() == name)
                        })
                        .unwrap()
                };
                let ordered = find(DeclarationKind::Function, Some("ordered"));
                let release = find(DeclarationKind::Implementation, None);
                let targeted =
                    lower_body(&receipt, module, &ordered.id, AnalysisLimits::default()).unwrap();
                assert_eq!(targeted.completion(), Completion::Checked);
                let mir = targeted.mir().unwrap();
                assert_eq!(o19_sequence(mir), expected, "test_mode {test_mode}");
                let disposer = mir.disposer(&release.id).unwrap();
                assert_eq!(o19_sequence(disposer), active, "test_mode {test_mode}");

                let report = analyze_ownership(&receipt, AnalysisLimits::default());
                let root = report
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == ordered.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked);
                assert_eq!(
                    o19_sequence(root.ordinary_mir().unwrap()),
                    expected,
                    "test_mode {test_mode}"
                );
                let [frame] = report.active_disposers() else {
                    panic!("one active disposer, test_mode {test_mode}");
                };
                assert_eq!(frame.site().node, release.node);
                assert_eq!(frame.ordinary_completion(), Completion::Checked);
                assert_eq!(
                    o19_sequence(frame.ordinary_mir().unwrap()),
                    active,
                    "test_mode {test_mode}"
                );
            }
        },
    );
}

/// Metering rule 11, ticket's mandated flip (ri-kdi): admitting a reference
/// member in a record with a custom disposer moves O19 from `Incomplete` at
/// `holder` (2,250/93/1 targeted, 187,190/4,193/5 graph) to `Checked`. The
/// old caps now give `ResourceLimit`; the row above holds the new caps and
/// `m5-acceptance/ri-kdi.md` the reason.
#[test]
fn o19_pre_admission_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::o19_pre_admission_caps_now_give_resource_limit",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", O19[0].text)).unwrap();
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
                let ordered = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some("ordered"))
                    .unwrap();
                let (old, _) = limits(statistics((2250, 93, 1)));
                let limited = lower_body(&receipt, module, &ordered.id, old).unwrap();
                crate::limit::assert_body_limit(&limited);
                let (old, _) = limits(statistics((187190, 4193, 5)));
                crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
            }
        },
    );
}

#[test]
fn o20_interpolation_reads_through_the_guard_owner_and_checks() {
    run(
        "rfc_cases::o20_interpolation_reads_through_the_guard_owner_and_checks",
        O20,
    );
}

#[test]
fn o21_guard_release_replacement_reports_r0500() {
    run_with(
        "rfc_cases::o21_guard_release_replacement_reports_r0500",
        O21,
        |index, graph, context| {
            if index == 3 {
                return;
            }
            if index != 0 {
                let (parent, _) = limits(statistics(O21_SIBLINGS_PARENT[index - 1]));
                let sibling = analyze_typed(graph, parent);
                crate::limit::assert_typed_limit(&sibling);
                assert!(sibling.receipt().is_none(), "{context}");
                assert!(sibling.bodies().findings().is_empty(), "{context}");
                return;
            }
            let (old, _) = limits(statistics(O21_PRE_REJECTION));
            let limited = analyze_typed(graph, old);
            crate::limit::assert_typed_limit(&limited);
            assert!(limited.receipt().is_none(), "{context}");
            assert!(limited.bodies().findings().is_empty(), "{context}");
        },
    );
}

#[test]
fn o22_descriptor_disposal_ends_only_that_descriptor_and_owner_disposal_rejects() {
    run_with(
        "rfc_cases::o22_descriptor_disposal_ends_only_that_descriptor_and_owner_disposal_rejects",
        O22,
        |index, graph, context| {
            if index != 0 {
                return;
            }
            let (old, _) = limits(statistics(O22_PRE_DESCRIPTOR_END));
            let limited = analyze_typed(graph, old);
            crate::limit::assert_typed_limit(&limited);
            assert!(limited.receipt().is_none(), "{context}");
            assert!(limited.bodies().findings().is_empty(), "{context}");
        },
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

/// O19's neighbours (ri-kdi). A holder's disposer reads its reference until
/// the holder's own cleanup, so a source that ends first, in a nested scope or
/// through an explicit `dispose`, is rejected (RFC 0007: cleanup does not
/// extend a source from an earlier-ending nested scope). Grouped and
/// loop-wrapped forms check. A custom-disposer record with an exclusive or a
/// two-level reference field stays unsupported, at the caps measured at the
/// parent revision 7002855e (metering rule 2).
const O19_NEIGHBOURS: &str = r#"
import std::core::dispose as release;
type Holder = { view: &string; }
implement release(value: Holder): void { let seen = value.view; }
type Exclusive = { view: &mut string; }
implement release(value: Exclusive): void {}
type Deep = { view: &&string; }
implement release(value: Deep): void {}
function scoped(seed: &string): void {
  let mut holder = Holder { view: seed };
  if (true) { let source = "inner"; holder = Holder { view: &source }; }
}
function early(seed: &string): void {
  let mut holder = Holder { view: seed };
  let source = "later";
  holder = Holder { view: &source };
  dispose(source);
}
function grouped(seed: &string): void {
  let mut holder = (Holder { view: (seed) });
  let source = "later";
  holder = (Holder { view: (&source) });
}
function looped(seed: &string, flag: bool): void {
  let mut holder = Holder { view: seed };
  let source = "later";
  while (flag) { holder = Holder { view: &source }; }
}
function exclusive(seed: string): void {
  let mut source = seed;
  let holder = Exclusive { view: &mut source };
}
function deep(seed: &string): void {
  let holder = Deep { view: &seed };
}
"#;

#[test]
fn o19_neighbours_reject_an_earlier_ending_source() {
    installed(
        "rfc_cases::o19_neighbours_reject_an_earlier_ending_source",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{O19_NEIGHBOURS}")).unwrap();
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
                check_receipt(
                    &format!("test_mode {test_mode}"),
                    &typed,
                    module,
                    (GraphCompletion::SourceRejected, (201497, 6580, 7)),
                    &[
                        Root {
                            name: "scoped",
                            want: Want::Rejected {
                                code: "R0203",
                                site: r####"{ let source = \"inner\"; holder = Holder { view: &source }; }"####,
                                related: r####"&source"####,
                            },
                            graph: None,
                            stats: (3399, 541, 7),
                        },
                        Root {
                            name: "early",
                            want: Want::Rejected {
                                code: "R0203",
                                site: r####"dispose(source)"####,
                                related: r####"&source"####,
                            },
                            graph: None,
                            stats: (3384, 522, 5),
                        },
                        Root {
                            name: "grouped",
                            want: Want::Checked,
                            graph: None,
                            stats: (6555, 897, 7),
                        },
                        Root {
                            name: "looped",
                            want: Want::Checked,
                            graph: None,
                            stats: (7086, 1138, 7),
                        },
                        Root {
                            name: "exclusive",
                            want: Want::Incomplete {
                                blocker: r####"holder"####,
                            },
                            graph: None,
                            stats: (2444, 185, 1),
                        },
                        Root {
                            name: "deep",
                            want: Want::Incomplete {
                                blocker: r####"holder"####,
                            },
                            graph: None,
                            stats: (2440, 182, 1),
                        },
                    ],
                );
            }
        },
    );
}
