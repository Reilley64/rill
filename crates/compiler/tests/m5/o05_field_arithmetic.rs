//! `i32` addition over field reads of a local record (ri-fmsk, review SP-F5).
//!
//! WIP, not mergeable: the RFC 0007 O5 `separate` function still stops
//! `Incomplete` at `*right += 2i32;`, a write through `&mut` that M5 does not
//! lower. See `m5-acceptance/ri-fmsk.md`.
use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};

const SIBLINGS: &str = r#"
type Pair = { left: i32; right: i32 };
type Wide = { left: i64; right: i64 };
type Outer = { inner: Pair; extra: i32 };
function nested(outer: Outer): i32 { return outer.inner.left + outer.extra; }
function nestedGrouped(outer: Outer): i32 { return ((outer.inner.left)) + ((outer.extra)); }
function wide(pair: Wide): i64 { return pair.left + pair.right; }
function wideGrouped(pair: Wide): i64 { return ((pair.left)) + ((pair.right)); }
function product(pair: Pair): i32 { return pair.left * 10i32 + pair.right; }
function literal(pair: Pair): i32 { return pair.left + 1i32; }
function lent(pair: &Pair): i32 { return pair.left + pair.right; }
function lentGrouped(pair: &Pair): i32 { return ((pair.left)) + ((pair.right)); }
"#;

const SUPPORTED: &str = r#"
type Pair = { left: i32; right: i32 };
function bare(): i32 {
  let pair = Pair { left: 1, right: 2 };
  return pair.left + pair.right;
}
"#;

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

/// Exact results measured at the parent 7002855e, before field operands lower.
/// The unsupported siblings still stop at their parent caps and sites.
#[test]
fn unsupported_field_arithmetic_keeps_its_parent_caps_and_sites() {
    probe(
        "o05_field_arithmetic::unsupported_field_arithmetic_keeps_its_parent_caps_and_sites",
        SIBLINGS,
        |_, module, receipt| {
            let graph_stats = statistics(195115, 4770, 5);
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
                ("nested", 2746, 223, 3, "outer.inner.left + outer.extra"),
                (
                    "nestedGrouped",
                    2746,
                    223,
                    3,
                    "((outer.inner.left)) + ((outer.extra))",
                ),
                ("wide", 2383, 164, 3, "pair.left + pair.right"),
                (
                    "wideGrouped",
                    2383,
                    164,
                    3,
                    "((pair.left)) + ((pair.right))",
                ),
                ("product", 2383, 164, 3, "pair.left * 10i32 + pair.right"),
                ("literal", 2383, 164, 3, "pair.left + 1i32"),
                ("lent", 2025, 105, 3, "pair.left + pair.right"),
                (
                    "lentGrouped",
                    2025,
                    105,
                    3,
                    "((pair.left)) + ((pair.right))",
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
                    expected.as_bytes(),
                    "{name}"
                );
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == decl.node)
                    .unwrap();
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

/// `pair.left + pair.right` over a local record lowers: two field copies into
/// scratch slots, the checked addition, then the export.
#[test]
fn field_addition_checks_with_exact_and_one_short_caps() {
    probe(
        "o05_field_arithmetic::field_addition_checks_with_exact_and_one_short_caps",
        SUPPORTED,
        |_, module, receipt| {
            let decl = named(module, "bare");
            let query = |cap| lower_body(receipt, module, &decl.id, cap).unwrap();
            let stats = statistics(2711, 328, 4);
            let (exact, short) = caps(stats);
            let target = query(exact);
            assert_eq!(target.completion(), Completion::Checked);
            assert_eq!(target.statistics(), stats);
            assert!(target.findings().is_empty() && target.blocked_at().is_none());
            let ops: Vec<_> = linear(target.mir().unwrap(), target.mir().unwrap().entry)
                .into_iter()
                .filter_map(|op| match op {
                    Operation::Copy { from, .. } => from
                        .field()
                        .map(|f| format!("copy {}", f.name.as_deref().unwrap_or("?"))),
                    Operation::CheckedI32Add { .. } => Some("add".to_string()),
                    _ => None,
                })
                .collect();
            assert_eq!(ops, ["copy left", "copy right", "add"]);
            for cap in short {
                let limited = query(cap);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none() && limited.findings().is_empty());
            }
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let root = graph
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == decl.node)
                .unwrap();
            assert_eq!(root.ordinary_completion(), Completion::Checked);
        },
    );
}
