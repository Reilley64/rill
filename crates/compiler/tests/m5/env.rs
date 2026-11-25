use super::*;
use compiler::analysis::typed::{PrimitiveOperation, TypedCompletion};
use compiler::loader::{Lookup, Module, Namespace};

const IMPORT: &str = "import std::env::{inputs, args};\n";

fn modes(request: &InstalledRequest) -> [InstalledRequest; 2] {
    [false, true].map(|test_mode| InstalledRequest {
        test_mode,
        ..request.clone()
    })
}

fn env_module(graph: &loader::Graph) -> Option<&Module> {
    graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Standard && m.key.path == "env.rill")
}

fn standard_modules(graph: &loader::Graph) -> usize {
    graph
        .modules()
        .iter()
        .filter(|m| m.key.origin == Origin::Standard)
        .count()
}

#[test]
fn env_rill_loads_only_when_a_program_imports_std_env() {
    const CASES: [(&str, bool); 11] = [
        ("", false),
        ("import std::{Result};\n", false),
        ("import std::{Option, Result};\n", false),
        ("import std::option;\n", false),
        ("import std;\n", false),
        ("import std::log;\n", false),
        ("import std::env::inputs;\n", true),
        ("import std::env::args;\n", true),
        (IMPORT, true),
        ("import std::env;\n", true),
        ("import std::log;\nimport std::env::args;\n", true),
    ];
    installed(
        "env::env_rill_loads_only_when_a_program_imports_std_env",
        |request| {
            for (source, loads) in CASES {
                fs::write(&request.entry, source).unwrap();
                for request in modes(&request) {
                    let test_mode = request.test_mode;
                    let context = format!("{source:?} test_mode {test_mode}");
                    let graph = loader::load_installed(request).expect(&context);
                    assert_eq!(env_module(&graph).is_some(), loads, "{context}");
                    let output = source.contains("log") || source == "import std;\n";
                    assert_eq!(
                        standard_modules(&graph),
                        17 + usize::from(loads) + usize::from(output),
                        "{context}"
                    );
                }
            }
        },
    );
}

#[test]
fn importing_std_env_adds_exactly_the_declarations_of_env_rill() {
    installed(
        "env::importing_std_env_adds_exactly_the_declarations_of_env_rill",
        |request| {
            fs::write(&request.entry, "").unwrap();
            let without = loader::load_installed(request.clone())
                .unwrap()
                .declaration_count();
            fs::write(&request.entry, IMPORT).unwrap();
            let graph = loader::load_installed(request).unwrap();
            let module = env_module(&graph).unwrap();
            assert_eq!(
                graph.declaration_count() - without,
                module.declarations.len()
            );
            assert!(!module.declarations.is_empty());
        },
    );
}

#[test]
fn std_env_publishes_only_inputs_args_and_their_two_types() {
    installed(
        "env::std_env_publishes_only_inputs_args_and_their_two_types",
        |request| {
            fs::write(&request.entry, IMPORT).unwrap();
            let graph = loader::load_installed(request).unwrap();
            let module = env_module(&graph).unwrap();
            let mut public: Vec<_> = module
                .declarations
                .iter()
                .filter(|d| d.public)
                .filter_map(|d| d.name.as_deref())
                .collect();
            public.sort_unstable();
            assert_eq!(public, ["EnvError", "Inputs", "args", "inputs"]);
            for name in ["inputs", "args"] {
                assert!(
                    matches!(
                        graph.lookup_export(&module.key, Namespace::ValueFunction, &[name]),
                        Lookup::Found(_)
                    ),
                    "{name}"
                );
            }
        },
    );
}

#[test]
fn a_tampered_env_rill_is_rejected_against_the_toolchain_pin() {
    installed(
        "env::a_tampered_env_rill_is_rejected_against_the_toolchain_pin",
        |request| {
            let stdlib = request
                .entry
                .ancestors()
                .nth(2)
                .unwrap()
                .join("lib/rill/std");
            let path = stdlib.join("env.rill");
            let mut bytes = fs::read(&path).unwrap();
            bytes.extend_from_slice(b"function other(): void {}\n");
            fs::write(&path, bytes).unwrap();
            fs::write(&request.entry, "").unwrap();
            loader::load_installed(request.clone()).unwrap();
            fs::write(&request.entry, IMPORT).unwrap();
            let error = loader::load_installed(request).unwrap_err();
            assert!(
                error.message.contains("differs from toolchain pin"),
                "{error:?}"
            );
        },
    );
}

fn typed_checked(
    request: &InstalledRequest,
    source: &str,
    check: impl Fn(&loader::Graph, &TypedReport<'_>),
) {
    fs::write(&request.entry, source).unwrap();
    for request in modes(request) {
        let graph = loader::load_installed(request).unwrap();
        let typed = analyze_typed(&graph, AnalysisLimits::default());
        assert_eq!(typed.completion(), TypedCompletion::Checked, "{source:?}");
        check(&graph, &typed);
    }
}

#[test]
fn inputs_and_args_are_typed_checked_and_their_bridges_bind_to_operations() {
    installed(
        "env::inputs_and_args_are_typed_checked_and_their_bridges_bind_to_operations",
        |request| {
            typed_checked(&request, IMPORT, |graph, typed| {
                let module = env_module(graph).unwrap();
                let bridged: Vec<_> = module
                    .declarations
                    .iter()
                    .filter_map(|d| {
                        typed
                            .env_bridge(&d.id)
                            .map(|operation| (d.name.as_deref().unwrap(), operation))
                    })
                    .collect();
                assert_eq!(
                    bridged,
                    [
                        ("captureInputs", PrimitiveOperation::CaptureInputs),
                        ("decodeArguments", PrimitiveOperation::DecodeArguments),
                    ]
                );
            });
        },
    );
}

#[test]
fn a_program_that_calls_inputs_and_args_is_typed_checked() {
    installed(
        "env::a_program_that_calls_inputs_and_args_is_typed_checked",
        |request| {
            for source in [
                "import std::env::{inputs, args};\nlet i = inputs();\nlet _ = i;\n",
                "import std::env::{inputs, args};\nlet _ = inputs();\n",
                "import std::env::{inputs, args, Inputs};\nfunction run(inputs: &Inputs): void { let _ = args(inputs); }\n",
            ] {
                typed_checked(&request, source, |_, _| {});
            }
        },
    );
}

#[test]
fn the_env_bridges_are_screened_before_they_are_charged_and_charged_by_key() {
    installed(
        "env::the_env_bridges_are_screened_before_they_are_charged_and_charged_by_key",
        |request| {
            fs::write(&request.entry, IMPORT).unwrap();
            for request in modes(&request) {
                let test_mode = request.test_mode;
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                assert_eq!(typed.completion(), TypedCompletion::Checked, "{test_mode}");
                let used = typed.statistics();
                crate::limit::assert_triple(
                    (used.work, used.entries, used.max_depth),
                    (444_459, 48_983, 12),
                    test_mode,
                );
                let exact = AnalysisLimits {
                    max_work: used.work,
                    max_entries: used.entries,
                    max_depth: used.max_depth,
                };
                let replay = analyze_typed(&graph, exact);
                assert_eq!(replay.completion(), TypedCompletion::Checked, "{test_mode}");
                for short in [
                    AnalysisLimits {
                        max_work: exact.max_work - 1,
                        ..exact
                    },
                    AnalysisLimits {
                        max_entries: exact.max_entries - 1,
                        ..exact
                    },
                ] {
                    let limited = analyze_typed(&graph, short);
                    crate::limit::assert_typed_limit(&limited);
                    assert!(limited.receipt().is_none(), "{test_mode}");
                }
            }
        },
    );
}
