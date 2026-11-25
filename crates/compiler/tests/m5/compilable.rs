use super::roots::fixture_with_main;
use super::*;
use compiler::analysis::compilable::{
    Blocker, CheckedBody, Compilability, CompilableProgram, ProgramAnalysis, UNSUPPORTED_SHAPE,
    compilable_program,
};
use compiler::analysis::ownership::{
    DependencyContract, GraphCompletion, GraphReport, analyze_ownership,
};
use compiler::analysis::scalars::BodyOwner;
use compiler::analysis::typed::{TypedCompletion, TypedReport};

const READY: &str = "function ready(flag: bool): bool { return flag; }\n";
const MINUS: &str = "function minus(a: i32, b: i32): i32 { return a - b; }\n";
const FORWARD: &str =
    "function forward(job: once () => [&bool, bool]): once () => [&bool, bool] { return job; }
function after(seed: bool): void {
    let other = true;
    let held = forward(() => [&seed, other]);
    let _ = held;
}
";
const REUSE: &str =
    "function reuse(value: string): bool { let taken = value; let again = value; return true; }\n";
const CUSTOM: &str = "import std::core::dispose as release;
type Custom = { keep: string; }
implement release(value: Custom): void {}
";

fn check(
    test: &str,
    program: &str,
    helper: &str,
    verify: impl for<'g> Fn(&'g loader::Graph, &GraphReport<'g>),
) {
    let main = format!("{}\n{program}", super::IMPORTS);
    fixture_with_main(test, &main, &[helper], |_, request, graph, receipt, _| {
        if request.test_mode {
            return;
        }
        verify(
            graph,
            &analyze_ownership(receipt, AnalysisLimits::default()),
        );
    });
}

fn blocked<'r, 'g>(result: Compilability<'r, 'g>) -> Blocker<'r, 'g> {
    match result {
        Compilability::Blocked(blocker) => blocker,
        Compilability::Compilable(program) => panic!("compilable: {program:?}"),
    }
}

fn compilable<'r, 'g>(result: Compilability<'r, 'g>) -> CompilableProgram<'r, 'g> {
    match result {
        Compilability::Compilable(program) => program,
        Compilability::Blocked(blocker) => panic!("blocked: {blocker:?}"),
    }
}

fn starts(body: &CheckedBody<'_, '_>) -> String {
    let bytes = body.site().span.bytes();
    let end = bytes.iter().position(|&b| b == b'(').unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn root_of<'r, 'g>(
    report: &'r GraphReport<'g>,
    prefix: &[u8],
) -> &'r compiler::analysis::ownership::RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| {
            root.site().module.path == "main.rill" && root.site().span.bytes().starts_with(prefix)
        })
        .unwrap()
}

#[test]
fn a_checked_startup_reaching_only_checked_functions_is_compilable_although_the_graph_is_incomplete()
 {
    let program = format!("{READY}let ok = ready(true);\n");
    check(
        "compilable::a_checked_startup_reaching_only_checked_functions_is_compilable_although_the_graph_is_incomplete",
        &program,
        "",
        |graph, report| {
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            assert!(
                report
                    .roots()
                    .iter()
                    .any(|root| root.completion() == Completion::Incomplete)
            );
            let program = compilable(compilable_program(graph, report));
            assert_eq!(program.startup().owner(), BodyOwner::Startup);
            assert_eq!(program.startup().site().module.path, "main.rill");
            let functions: Vec<_> = program.functions().iter().map(starts).collect();
            assert_eq!(functions, ["function ready"]);
            assert!(program.active_disposers().is_empty());
            let ready = root_of(report, b"function ready(");
            assert_eq!(
                program.functions()[0].mir().entry,
                ready.mir().unwrap().entry
            );
        },
    );
}

#[test]
fn an_unreachable_incomplete_function_does_not_block() {
    let program = format!("{READY}{MINUS}let ok = ready(true);\n");
    check(
        "compilable::an_unreachable_incomplete_function_does_not_block",
        &program,
        "",
        |graph, report| {
            let minus = root_of(report, b"function minus(");
            assert_eq!(minus.completion(), Completion::Incomplete);
            assert_eq!(minus.blocked_at().unwrap().span.bytes(), b"a - b");
            let program = compilable(compilable_program(graph, report));
            let functions: Vec<_> = program.functions().iter().map(starts).collect();
            assert_eq!(functions, ["function ready"]);
        },
    );
}

#[test]
fn a_reachable_incomplete_function_blocks_with_its_span() {
    let program = format!("{READY}{MINUS}let ok = ready(true);\nlet n = minus(3i32, 1i32);\n");
    check(
        "compilable::a_reachable_incomplete_function_blocks_with_its_span",
        &program,
        "",
        |graph, report| {
            let Blocker::Incomplete { site } = blocked(compilable_program(graph, report)) else {
                panic!("expected Incomplete")
            };
            assert_eq!(site.module.path, "main.rill");
            assert_eq!(site.span.bytes(), b"a - b");
        },
    );
}

#[test]
fn a_body_reached_only_through_a_callable_transfer_blocks_although_the_startup_is_checked() {
    let program = format!("{FORWARD}after(true);\n");
    check(
        "compilable::a_body_reached_only_through_a_callable_transfer_blocks_although_the_startup_is_checked",
        &program,
        "",
        |graph, report| {
            let startup = report
                .roots()
                .iter()
                .find(|root| {
                    root.owner() == BodyOwner::Startup && root.site().module.path == "main.rill"
                })
                .unwrap();
            assert_eq!(startup.completion(), Completion::Checked);
            let after = root_of(report, b"function after(");
            assert_eq!(after.completion(), Completion::Checked);
            assert!(matches!(
                after.dependencies(),
                [edge] if matches!(edge.contract, DependencyContract::CallableTransfer(_))
            ));
            let forward = root_of(report, b"function forward(");
            assert_eq!(forward.completion(), Completion::Incomplete);
            let Blocker::Incomplete { site } = blocked(compilable_program(graph, report)) else {
                panic!("expected Incomplete")
            };
            assert_eq!(
                site.span.bytes(),
                forward.blocked_at().unwrap().span.bytes()
            );
            assert_eq!(
                site.span.start(),
                forward.blocked_at().unwrap().span.start()
            );
        },
    );
}

#[test]
fn the_same_transfer_is_compilable_when_nothing_reaches_it() {
    let program = format!("{FORWARD}let ok = true;\n");
    check(
        "compilable::the_same_transfer_is_compilable_when_nothing_reaches_it",
        &program,
        "",
        |graph, report| {
            let program = compilable(compilable_program(graph, report));
            assert!(program.functions().is_empty());
        },
    );
}

#[test]
fn a_reachable_custom_disposer_is_an_active_body_of_the_program() {
    let program = format!("{CUSTOM}let held = Custom {{ keep: \"kept\" }};\n");
    check(
        "compilable::a_reachable_custom_disposer_is_an_active_body_of_the_program",
        &program,
        "",
        |graph, report| {
            assert_eq!(
                root_of(report, b"implement release(").completion(),
                Completion::Incomplete
            );
            let program = compilable(compilable_program(graph, report));
            assert!(program.functions().is_empty());
            let [active] = program.active_disposers() else {
                panic!("one active disposer: {program:?}")
            };
            assert_eq!(starts(active), "implement release");
            assert_eq!(active.owner(), report.active_disposers()[0].owner());
        },
    );
}

#[test]
fn an_imported_modules_startup_is_not_part_of_the_program() {
    let helper = "export function ready(): bool { return true; }\nlet bad = 3i32 - 1i32;\n";
    let program = "import helper::ready;\nlet ok = ready();\n";
    check(
        "compilable::an_imported_modules_startup_is_not_part_of_the_program",
        program,
        helper,
        |graph, report| {
            let imported = report
                .roots()
                .iter()
                .find(|root| {
                    root.owner() == BodyOwner::Startup && root.site().module.path == "helper.rill"
                })
                .unwrap();
            assert_eq!(imported.completion(), Completion::Incomplete);
            let program = compilable(compilable_program(graph, report));
            let functions: Vec<_> = program
                .functions()
                .iter()
                .map(|body| body.site().module.path.clone())
                .collect();
            assert_eq!(functions, ["helper.rill"]);
        },
    );
}

#[test]
fn a_finding_blocks_as_itself_even_in_an_unreachable_body() {
    let program = format!("{READY}{REUSE}let ok = ready(true);\n");
    check(
        "compilable::a_finding_blocks_as_itself_even_in_an_unreachable_body",
        &program,
        "",
        |graph, report| {
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            let Blocker::SourceRejected { finding } = blocked(compilable_program(graph, report))
            else {
                panic!("expected SourceRejected")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"value");
            assert!(std::ptr::eq(finding, &report.findings()[0]));
        },
    );
}

#[test]
fn a_resource_limit_blocks_as_itself_on_every_axis() {
    let program = format!("{READY}let ok = ready(true);\n");
    let main = format!("{}\n{program}", super::IMPORTS);
    fixture_with_main(
        "compilable::a_resource_limit_blocks_as_itself_on_every_axis",
        &main,
        &[""],
        |_, request, graph, receipt, _| {
            if request.test_mode {
                return;
            }
            let full = analyze_ownership(receipt, AnalysisLimits::default());
            let stats = full.statistics();
            compilable(compilable_program(graph, &full));
            let exact = AnalysisLimits {
                max_work: stats.work,
                max_entries: stats.entries,
                max_depth: stats.max_depth,
            };
            compilable(compilable_program(
                graph,
                &analyze_ownership(receipt, exact),
            ));
            for short in [
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
            ] {
                let limited = analyze_ownership(receipt, short);
                crate::limit::assert_graph_limit(&limited);
                let Blocker::ResourceLimit { statistics } =
                    blocked(compilable_program(graph, &limited))
                else {
                    panic!("expected ResourceLimit")
                };
                assert_eq!(statistics, limited.statistics());
            }
        },
    );
}

#[test]
fn the_unsupported_shape_code_is_in_the_registry_and_the_contract() {
    assert_eq!(UNSUPPORTED_SHAPE, "R0605");
    let registry = include_str!("../../../../docs/diagnostic-codes.md");
    assert!(registry.contains("### R0605. "));
    let contract = include_str!("../../../../docs/diagnostic-contracts.md");
    assert!(contract.contains("R0605"));
}

fn analysis(
    test: &str,
    program: &str,
    limits: AnalysisLimits,
    verify: impl for<'g, 't> Fn(&'t TypedReport<'g>, Compilability<'t, 'g>),
) {
    installed(test, |request| {
        fs::write(&request.entry, program).unwrap();
        let graph = loader::load_installed(request).unwrap();
        let typed = analyze_typed(&graph, limits);
        let analysis = ProgramAnalysis::new(&graph, &typed, limits);
        verify(&typed, analysis.compilability());
    });
}

#[test]
fn the_program_analysis_of_a_typed_receipt_is_the_compilable_program_rule() {
    let program = format!("{}\n{READY}let ok = ready(true);\n", super::IMPORTS);
    analysis(
        "compilable::the_program_analysis_of_a_typed_receipt_is_the_compilable_program_rule",
        &program,
        AnalysisLimits::default(),
        |typed, result| {
            assert!(typed.receipt().is_some());
            let program = compilable(result);
            let functions: Vec<_> = program.functions().iter().map(starts).collect();
            assert_eq!(functions, ["function ready"]);
        },
    );
}

#[test]
fn the_program_analysis_blocks_on_an_ownership_blocker() {
    let program = format!(
        "{}\n{READY}{MINUS}let ok = ready(true);\nlet n = minus(3i32, 1i32);\n",
        super::IMPORTS
    );
    analysis(
        "compilable::the_program_analysis_blocks_on_an_ownership_blocker",
        &program,
        AnalysisLimits::default(),
        |_, result| {
            let Blocker::Incomplete { site } = blocked(result) else {
                panic!("expected Incomplete")
            };
            assert_eq!(site.span.bytes(), b"a - b");
        },
    );
}

#[test]
fn a_typed_finding_blocks_as_a_located_typed_rejection() {
    analysis(
        "compilable::a_typed_finding_blocks_as_a_located_typed_rejection",
        "function f(): bool { return 1i32; }\nfunction g(): bool { return 2i32; }\n",
        AnalysisLimits::default(),
        |typed, result| {
            assert!(typed.receipt().is_none());
            let Blocker::TypedRejected { findings } = blocked(result) else {
                panic!("expected TypedRejected")
            };
            let found: Vec<_> = findings
                .iter()
                .map(|finding| (finding.code, finding.site.span.bytes()))
                .collect();
            assert_eq!(
                found,
                [(Some("R0100"), &b"1i32"[..]), (Some("R0100"), &b"2i32"[..])]
            );
        },
    );
}

#[test]
fn a_shape_typed_analysis_cannot_check_blocks_at_its_site() {
    analysis(
        "compilable::a_shape_typed_analysis_cannot_check_blocks_at_its_site",
        "let flag: bool = true;\nlet n: i32 = flag;\n",
        AnalysisLimits::default(),
        |typed, result| {
            assert!(typed.receipt().is_none());
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            let Blocker::Incomplete { site } = blocked(result) else {
                panic!("expected Incomplete")
            };
            assert_eq!(site.module.path, "main.rill");
            assert_eq!(site.span.bytes(), b"let n: i32 = flag;");
        },
    );
}

#[test]
fn a_typed_resource_limit_blocks_as_itself_with_the_typed_statistics() {
    let tight = AnalysisLimits {
        max_work: 10,
        ..AnalysisLimits::default()
    };
    analysis(
        "compilable::a_typed_resource_limit_blocks_as_itself_with_the_typed_statistics",
        "let ok = true;\n",
        tight,
        |typed, result| {
            assert_eq!(typed.completion(), TypedCompletion::ResourceLimit);
            let Blocker::ResourceLimit { statistics } = blocked(result) else {
                panic!("expected ResourceLimit")
            };
            assert_eq!(statistics, typed.statistics());
        },
    );
}

#[test]
fn hello_world_is_compilable_with_the_log_body_among_its_functions_in_both_modes() {
    installed(
        "compilable::hello_world_is_compilable_with_the_log_body_among_its_functions_in_both_modes",
        |request| {
            for source in [
                "import std::log;\n\nlog(\"hello world\");\n",
                "import std::log;\nlet s = \"x\";\nlog(&s);\n",
            ] {
                fs::write(&request.entry, source).unwrap();
                for test_mode in [false, true] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let graph = loader::load_installed(request).unwrap();
                    let limits = AnalysisLimits::default();
                    let typed = analyze_typed(&graph, limits);
                    assert!(typed.receipt().is_some());
                    let analysis = ProgramAnalysis::new(&graph, &typed, limits);
                    let program = compilable(analysis.compilability());
                    assert_eq!(program.startup().site().module.path, "main.rill");
                    let functions: Vec<_> = program
                        .functions()
                        .iter()
                        .map(|body| (body.site().module.path.clone(), starts(body)))
                        .collect();
                    assert_eq!(
                        functions,
                        [("output.rill".to_owned(), "implement log".to_owned())]
                    );
                    assert!(program.active_disposers().is_empty());
                }
            }
        },
    );
}
