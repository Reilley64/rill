//! RFC 0007 O5 (ri-fmsk, review SP-F5): `i32` addition over field reads of a
//! local record, and `*name += literal` through a local `&mut record.field`.
//! The O5 row itself is in `rfc_cases`; this file pins the unsupported
//! siblings at their parent caps and the supported controls.
//! See `m5-acceptance/ri-fmsk.md`.
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

/// Writes `*name += literal` through a local reference that is not an
/// exclusive reborrow of one `i32` field of a local record, and whole-record
/// loans that are used. Each keeps the parent's caps and blocker.
const WRITE_SIBLINGS: &str = r#"
type Pair = { left: i32; right: i32 };
type Outer = { inner: Pair; extra: i32 };
function touch(pair: &mut Pair): void {}
function localScalar(): i32 {
  let mut value = 1i32;
  let view = &mut value;
  *view += 1i32;
  return value;
}
function nestedField(): i32 {
  let mut outer = Outer { inner: Pair { left: 1, right: 2 }, extra: 3 };
  let view = &mut outer.inner.left;
  *view += 1i32;
  return outer.extra;
}
function groupedTarget(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  (*left) += 1i32;
  return pair.right;
}
function groupedDescriptor(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  *(left) += 1i32;
  return pair.right;
}
function groupedOperand(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  *left += (1i32);
  return pair.right;
}
function computedOperand(step: i32): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  *left += step;
  return pair.right;
}
function throughWholeLoan(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let whole = &mut pair;
  let left = &mut whole.left;
  *left += 1i32;
  return 1i32;
}
function wholeLoanCalled(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let whole = &mut pair;
  let left = &mut pair.left;
  *left += 1i32;
  touch(whole);
  return 1i32;
}
function wholeLoanRead(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let whole = &mut pair;
  let left = &mut pair.left;
  *left += 1i32;
  let right = whole.right;
  return right;
}
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

type Row = (&'static str, usize, usize, usize, &'static str);

/// Each row is a function that stays `Incomplete` at its parent caps and
/// blocker, exact and one short, as a targeted query and as the graph root; the
/// module's graph keeps its parent caps too. Measured in a `git worktree add`
/// of the parent before the source change.
fn keeps_parent_caps(test: &str, source: &'static str, graph_stats: Statistics, rows: &[Row]) {
    probe(test, source, |_, module, receipt| {
        check_incomplete_rows(module, receipt, graph_stats, rows);
    });
}

fn check_incomplete_rows<'g>(
    module: &'g loader::Module,
    receipt: &TypedReceipt<'_, 'g>,
    graph_stats: Statistics,
    rows: &[Row],
) {
    let (exact, short) = caps(graph_stats);
    let graph = analyze_ownership(receipt, exact);
    assert_eq!(graph.completion(), GraphCompletion::Incomplete);
    assert_eq!(graph.statistics(), graph_stats);
    for cap in short {
        crate::limit::assert_graph_limit(&analyze_ownership(receipt, cap));
    }
    for &(name, work, entries, depth, expected) in rows {
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
            crate::limit::assert_body_limit(&limited);
        }
    }
}

/// Measured at the parent 7002855e, before field operands lower.
#[test]
fn unsupported_field_arithmetic_keeps_its_parent_caps_and_sites() {
    keeps_parent_caps(
        "o05_field_arithmetic::unsupported_field_arithmetic_keeps_its_parent_caps_and_sites",
        SIBLINGS,
        statistics(195115, 4770, 5),
        &[
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
        ],
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
                crate::limit::assert_body_limit(&limited);
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

/// Measured at the parent f24d825b (the merge of the base into this branch,
/// before the local field descriptor lowers); `touch` is a Checked control.
#[test]
fn unsupported_field_reborrow_writes_keep_their_parent_caps_and_sites() {
    keeps_parent_caps(
        "o05_field_arithmetic::unsupported_field_reborrow_writes_keep_their_parent_caps_and_sites",
        WRITE_SIBLINGS,
        statistics(201135, 5074, 5),
        &[
            ("localScalar", 2203, 196, 3, "*view += 1i32;"),
            ("nestedField", 2931, 314, 3, "*view += 1i32;"),
            ("groupedTarget", 2565, 255, 3, "(*left) += 1i32;"),
            ("groupedDescriptor", 2565, 255, 3, "*(left) += 1i32;"),
            ("groupedOperand", 2565, 255, 3, "*left += (1i32);"),
            ("computedOperand", 2571, 261, 3, "*left += step;"),
            ("throughWholeLoan", 2568, 260, 3, "*left += 1i32;"),
            ("wholeLoanCalled", 2587, 264, 3, "whole"),
            ("wholeLoanRead", 2584, 265, 3, "whole.right"),
        ],
    );
}

/// RFC 0007 O5 `separate`: both fields are borrowed exclusively before either
/// is written; each write reads through its own descriptor, adds, installs
/// through the same descriptor; the whole record reads after both.
#[test]
fn separate_borrows_both_fields_then_writes_through_each_descriptor() {
    probe(
        "o05_field_arithmetic::separate_borrows_both_fields_then_writes_through_each_descriptor",
        SEPARATE,
        |_, module, receipt| {
            let decl = named(module, "separate");
            let target = lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
            assert_eq!(target.completion(), Completion::Checked);
            assert!(target.findings().is_empty() && target.blocked_at().is_none());
            let mir = target.mir().unwrap();
            let mut installs = Vec::new();
            let ops: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter_map(|op| match op {
                    Operation::Borrow {
                        source,
                        exclusive: true,
                        ..
                    } => source
                        .field()
                        .map(|f| format!("borrow {}", f.name.as_deref().unwrap_or("?"))),
                    Operation::Dereference(_) => Some("read".to_string()),
                    Operation::CheckedI32Add { .. } => Some("add".to_string()),
                    Operation::InstallThrough { descriptor, .. } => {
                        installs.push(descriptor.root.node);
                        Some("install".to_string())
                    }
                    Operation::Copy { from, .. } => from
                        .field()
                        .map(|f| format!("copy {}", f.name.as_deref().unwrap_or("?"))),
                    _ => None,
                })
                .collect();
            assert_eq!(
                ops,
                [
                    "borrow left",
                    "borrow right",
                    "read",
                    "add",
                    "install",
                    "read",
                    "add",
                    "install",
                    "copy left",
                    "copy right",
                    "add"
                ]
            );
            assert_ne!(installs[0], installs[1], "one descriptor per field");
        },
    );
}

const SEPARATE: &str = r#"
type Pair = { left: i32; right: i32 };
function separate(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let right = &mut pair.right;
  *left += 1i32;
  *right += 2i32;
  return pair.left + pair.right;
}
"#;

/// A write through a local `&mut i32` binding that was not written
/// `&mut record.field` reaches `self.place(target)` before the slot flag
/// rejects it, so it pays one work unit that the parent did not (graph +1).
/// Metering rule 1 named exception (chief of staff, ruling of 2026-11-24
/// 14:47 AEDT, ST-9): the checks the review named after the charge (type,
/// literal) cannot fire at that site, and no paid fact says how the binding was
/// written. Each root keeps its outcome and blocker, pinned exact and one
/// short, as a targeted query and as the graph root; the parent caps
/// (`f24d825b`, before the route existed) give `ResourceLimit`. Pins of
/// current behaviour: they pass on the base, so rule 10's red-first does not
/// apply.
fn pays_one_work(
    test: &str,
    source: &'static str,
    graph: (Statistics, Statistics),
    row: Row,
    parent_row: Statistics,
) {
    probe(test, source, |_, module, receipt| {
        check_incomplete_rows(module, receipt, graph.0, &[row]);
        let decl = named(module, row.0);
        let (parent, _) = caps(parent_row);
        crate::limit::assert_body_limit(&lower_body(receipt, module, &decl.id, parent).unwrap());
        let (parent, _) = caps(graph.1);
        crate::limit::assert_graph_limit(&analyze_ownership(receipt, parent));
    });
}

#[test]
fn local_descriptor_write_through_another_binding_pays_one_work() {
    pays_one_work(
        "o05_field_arithmetic::local_descriptor_write_through_another_binding_pays_one_work",
        r#"
type Holder = { x: i32 };
function viaOther(p: &mut i32): void {
  let mut h = Holder { x: 1i32 };
  let e = &mut h.x;
  let d = p;
  *d += 1i32;
}
"#,
        (statistics(186742, 4198, 5), statistics(186741, 4198, 5)),
        ("viaOther", 2246, 103, 2, "*d += 1i32;"),
        statistics(2245, 103, 2),
    );
}

#[test]
fn local_descriptor_write_through_a_reborrow_pays_one_work() {
    pays_one_work(
        "o05_field_arithmetic::local_descriptor_write_through_a_reborrow_pays_one_work",
        r#"
type Holder = { x: i32 };
function viaReborrow(p: &mut i32): void {
  let mut h = Holder { x: 1i32 };
  let e = &mut h.x;
  let d = &mut *p;
  *d += 1i32;
}
"#,
        (statistics(186866, 4200, 5), statistics(186865, 4200, 5)),
        ("viaReborrow", 2250, 105, 2, "*d += 1i32;"),
        statistics(2249, 105, 2),
    );
}
