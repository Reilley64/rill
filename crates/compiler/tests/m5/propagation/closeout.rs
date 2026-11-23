//! Closeout of ri-vgz (`ri-vgz.8`, row C6). `wrapsAfterRhs` has targeted MIR
//! (`returning_wrap_rhs_forwards_instead_of_wrapping`) but no ordinary
//! graph-root MIR. The only reason is the new outer wrapper on the RHS
//! success path: its canonical cause display is the `RetainedErrorDisplay`
//! provider edge, which the graph keeps pending (bead `ri-ink`, deferred by
//! owner decision D7). The graph does not need the RHS call: the control
//! `terminalRhs` forwards instead of wrapping and has graph-root MIR.
use super::*;
use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};

const SOURCE: &str = r#"
    import std::result::Err as fail;
    function marker(): Result<i32, i32> {
        let seed = fail<i32, i32>(7i32);
        let value = seed ? 9i32;
        return Result.Ok(value);
    }
    function after(flag: bool): bool { return flag; }
    function wrapsAfterRhs(input: Result<bool, i32>, tail: string): Result<bool, i32> {
        let flag = input ? (marker()?);
        return Result.Ok(after(flag));
    }
    function scalarWrap(input: Result<bool, i32>, tail: string): Result<bool, i32> {
        let flag = input ? 3i32;
        return Result.Ok(flag);
    }
    function carryFailure(source: Result<i32, i32>.Err): Result<i32, i32>.Err {
        return source;
    }
    function terminalRhs(input: Result<bool, i32>.Err, source: Result<i32, i32>.Err, tail: string): Result<bool, i32> {
        let _ = input ? (carryFailure(source)?);
        return Result.Ok(after(true));
    }
"#;

#[test]
fn wraps_after_rhs_has_no_graph_root_mir_only_because_of_the_retained_display_edge() {
    probe(
        "propagation::closeout::wraps_after_rhs_has_no_graph_root_mir_only_because_of_the_retained_display_edge",
        SOURCE,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let root_of = |name: &str| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == decl.node)
                    .unwrap();
                (decl, root)
            };
            let blocked = |name: &str| {
                let (_, root) = root_of(name);
                root.blocked_at()
                    .map(|site| site.span.bytes().escape_ascii().to_string())
            };

            // Targeted MIR exists; the graph root has none and stops at the wrap.
            let (decl, root) = root_of("wrapsAfterRhs");
            let targeted =
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::Checked);
            assert!(targeted.mir().is_some());
            // Fixture-specific caps; the ledger's 12,574 / 2,472 / 6 is the larger
            // fixture of `returning_wrap_rhs_forwards_instead_of_wrapping`.
            let stats = targeted.statistics();
            assert_eq!(
                (stats.work, stats.entries, stats.max_depth),
                (11_666, 2_019, 6)
            );
            let exact = AnalysisLimits {
                max_work: stats.work,
                max_entries: stats.entries,
                max_depth: stats.max_depth,
            };
            let at = |limits| lower_body(receipt, module, &decl.id, limits).unwrap();
            assert!(at(exact).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
            ] {
                let limited = at(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            assert_eq!(root.ordinary_completion(), Completion::Incomplete);
            assert!(root.ordinary_mir().is_none());
            assert_eq!(
                blocked("wrapsAfterRhs").as_deref(),
                Some("input ? (marker()?)")
            );

            // The wrap itself is the cause: a scalar wrap with no RHS call stops at
            // the same pending edge, and a forwarding RHS (no new wrapper) checks.
            assert_eq!(blocked("scalarWrap").as_deref(), Some("input ? 3i32"));
            let (_, root) = root_of("terminalRhs");
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.ordinary_mir().is_some());

            // The graph is Incomplete, not rejected and not limited, with no finding.
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert!(graph.findings().is_empty());

            // Exact and one-short whole-graph caps, each axis independently.
            let s = graph.statistics();
            let exact = AnalysisLimits {
                max_work: s.work,
                max_entries: s.entries,
                max_depth: s.max_depth,
            };
            // Measured on this fixture, both modes: 203,035 / 7,345 / 6. With
            // the i32 equality source root (ri-785.9, metering rule 11, D2 (b))
            // it is 203,351 / 7,479 / 6, and the old cap gives ResourceLimit.
            assert_pre_equality_graph_cap(receipt, (203_035, 7_345, 6));
            assert_eq!((s.work, s.entries, s.max_depth), (203_351, 7_479, 6));
            let again = analyze_ownership(receipt, exact);
            assert_eq!(again.completion(), GraphCompletion::Incomplete);
            for short in [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
            ] {
                assert_eq!(
                    analyze_ownership(receipt, short).completion(),
                    GraphCompletion::ResourceLimit
                );
            }
        },
    );
}
