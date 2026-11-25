use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::typed::TypedCompletion;

type Placed = (loader::ModuleKey, usize, loader::DeclarationKind, usize);

fn placed(graph: &loader::Graph) -> Vec<Placed> {
    let mut placed: Vec<_> = graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .map(|declaration| {
            let id = &declaration.id;
            (id.module.clone(), id.start, id.kind, id.ordinal.index())
        })
        .collect();
    placed.sort();
    placed
}

#[test]
fn declaration_ordinals_are_dense_and_equal_in_both_load_modes() {
    installed(
        "declaration_ordinal::declaration_ordinals_are_dense_and_equal_in_both_load_modes",
        |request| {
            fs::write(
                &request.entry,
                r#"
                import helper::{Pair, total};
                import std::core::dispose as release;
                type Named = { text: string; count: i32; }
                enum Shape { Dot, Line(i32), Box { width: i32 } }
                implement release(value: Named): void {}
                function pick(shape: Shape, pair: Pair): i32 {
                    let start = total(pair);
                    for (let item of [1, 2]) { let seen = item; }
                    let Shape.Line(left) = shape;
                    return start;
                }
                "#,
            )
            .unwrap();
            fs::write(
                request.entry.with_file_name("helper.rill"),
                r#"
                export type Pair = { low: i32; high: i32; }
                export function total(pair: Pair): i32 { let sum = pair.low; return sum; }
                "#,
            )
            .unwrap();
            let loaded: Vec<_> = [false, true]
                .into_iter()
                .map(|test_mode| {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    (placed(&graph), graph.declaration_count())
                })
                .collect();
            assert_eq!(loaded[0], loaded[1]);
            let (placed, count) = &loaded[0];
            let mut ordinals: Vec<_> = placed.iter().map(|&(.., ordinal)| ordinal).collect();
            ordinals.sort_unstable();
            assert_eq!(ordinals, (0..*count).collect::<Vec<_>>());
            assert!(*count > 100);
        },
    );
}

#[test]
fn the_declaration_table_charges_one_work_and_one_entry_per_declaration() {
    const TEXT: &str = r####"
type Named = { x: i32; label: string };
function named(): Named { return Named { x: 1, label: "a" }; }
function textRead(): i32 { return (&named()).x; }
function textBind(): void { let view = &named(); }
"####;
    installed(
        "declaration_ordinal::the_declaration_table_charges_one_work_and_one_entry_per_declaration",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{TEXT}")).unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let (work, entries, max_depth) = (449687, 48501, 12);
                let at = |max_work, max_entries| {
                    analyze_typed(
                        &graph,
                        AnalysisLimits {
                            max_work,
                            max_entries,
                            max_depth,
                        },
                    )
                };
                let exact = crate::limit::measured(
                    "analyze_typed",
                    "exact",
                    AnalysisLimits {
                        max_work: work,
                        max_entries: entries,
                        max_depth,
                    },
                    |limits| analyze_typed(&graph, limits),
                    |report| report.statistics(),
                );
                assert_eq!(exact.completion(), TypedCompletion::Checked);
                crate::limit::assert_stats(
                    exact.statistics(),
                    Statistics {
                        work,
                        entries,
                        max_depth,
                    },
                    "",
                );
                let count = graph.declaration_count();
                for limited in [
                    at(work - count, entries),
                    at(work, entries - count),
                    at(work - count, entries - count),
                ] {
                    crate::limit::assert_typed_limit(&limited);
                }
            }
        },
    );
}
