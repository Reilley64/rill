use super::*;
use compiler::analysis::scalars::BodyOwner;
use compiler::analysis::{
    QueryError, Statistics,
    ownership::{Report, lower_root},
};
use compiler::loader::{Declaration, DeclarationKind};

pub(super) const MAIN: &str = r#"import std::{List, Result, Err};
import std::channel::{Receiver, Sender};
import std::convert::{into, tryInto};
import std::error::formatError;
import std::iter::iterate;
import std::mutex::LockGuard;
import std::number::ParseError;
import std::option::{Option, map as mapOption};
import std::result::{isOk, map as mapResult};
import std::shared::Shared;
import helper::ready;
function entryControl(flag: bool): bool { return flag; }
expect ready();
"#;
pub(super) const HELPER: &str = r#"export function ready(): bool { return true; }
function unused(value: string): bool;
implement unused(value: string): bool { let taken = value;  return true; }
function retainedControl(value: string): bool { let taken = value;  return true; }
expect retainedControl("expectation");
let text = "startup";
let taken = text;
"#;

pub(super) fn fixture(
    test: &str,
    sources: &[&str],
    verify: impl for<'g> FnMut(
        usize,
        &InstalledRequest,
        &'g loader::Graph,
        &TypedReceipt<'_, 'g>,
        &TypedReport<'g>,
    ),
) {
    fixture_with_main(test, MAIN, sources, verify);
}

pub(super) fn fixture_with_main(
    test: &str,
    main: &str,
    sources: &[&str],
    mut verify: impl for<'g> FnMut(
        usize,
        &InstalledRequest,
        &'g loader::Graph,
        &TypedReceipt<'_, 'g>,
        &TypedReport<'g>,
    ),
) {
    installed(test, |request| {
        fs::write(&request.entry, main).unwrap();
        for (index, source) in sources.iter().enumerate() {
            fs::write(request.entry.with_file_name("helper.rill"), source).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request.clone()).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().unwrap_or_else(|| {
                    panic!(
                        "installed M4 handoff: {:?}, {:?}",
                        typed.completion(),
                        typed.core_blockers()
                    )
                });
                verify(index, &request, &graph, &receipt, &typed);
            }
        }
    });
}

fn app_modules(graph: &loader::Graph) -> (&loader::Module, &loader::Module) {
    let module = |path| {
        graph
            .modules()
            .iter()
            .find(|m| m.key.origin == Origin::Application && m.key.path == path)
            .unwrap()
    };
    (module("main.rill"), module("helper.rill"))
}

fn named<'g>(module: &'g loader::Module, name: &str) -> &'g Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
}

fn checked_entry(receipt: &TypedReceipt<'_, '_>, main: &loader::Module) {
    let entry = named(main, "entryControl");
    let report = lower_body(receipt, main, &entry.id, AnalysisLimits::default()).unwrap();
    assert_eq!(report.completion(), Completion::Checked);
    assert!(report.mir().is_some());
}

fn assert_reuse(report: &Report<'_>, helper: &loader::Module, source: &str, owner: &str) {
    assert_eq!(
        report.completion(),
        Completion::SourceRejected,
        "{report:?}"
    );
    assert!(report.mir().is_none() && report.blocked_at().is_none());
    let [finding] = report.findings() else {
        panic!("{report:?}")
    };
    assert_eq!(finding.code(), "R0200");
    assert_eq!(finding.site.module, &helper.key);
    assert_eq!(finding.related.module, &helper.key);
    assert_eq!(finding.site.span.bytes(), owner.as_bytes());
    assert_eq!(finding.related.span.bytes(), owner.as_bytes());
    assert_eq!(
        finding.site.span.start(),
        source.find(&format!("again = {owner}")).unwrap() + "again = ".len()
    );
    assert_eq!(
        finding.related.span.start(),
        source[..finding.site.span.start()]
            .rfind(&format!("taken = {owner}"))
            .unwrap()
            + "taken = ".len()
    );
}

pub(super) fn assert_limits(
    receipt: &TypedReceipt<'_, '_>,
    module: &loader::Module,
    owner: BodyOwner<'_>,
    report: &Report<'_>,
    (work, entries, max_depth): (usize, usize, usize),
) {
    let stats = Statistics {
        work,
        entries,
        max_depth,
    };
    crate::limit::assert_stats(report.statistics(), stats, "");
    if crate::limit::reporting() {
        return;
    }
    let limits = AnalysisLimits {
        max_work: work,
        max_entries: entries,
        max_depth,
    };
    let exact = lower_root(receipt, module, owner, limits).unwrap();
    crate::limit::assert_stats(exact.statistics(), stats, "");
    assert_eq!(exact.completion(), report.completion());
    assert_eq!(exact.mir().is_some(), report.mir().is_some());
    assert_eq!(exact.findings().len(), report.findings().len());
    for (finding, expected) in exact.findings().iter().zip(report.findings()) {
        assert_eq!(finding.code(), expected.code());
        assert_eq!(finding.site.span, expected.site.span);
        assert_eq!(finding.related.span, expected.related.span);
    }
    for short in [
        AnalysisLimits {
            max_work: work - 1,
            ..limits
        },
        AnalysisLimits {
            max_entries: entries - 1,
            ..limits
        },
        AnalysisLimits {
            max_depth: max_depth - 1,
            ..limits
        },
    ] {
        let result = lower_root(receipt, module, owner, short).unwrap();
        crate::limit::assert_body_limit(&result);
        assert!(
            result.findings().is_empty() && result.mir().is_none() && result.blocked_at().is_none()
        );
    }
}

#[test]
fn imported_startup_is_a_lexical_root_in_both_modes() {
    let test = "roots::imported_startup_is_a_lexical_root_in_both_modes";
    let reuse = format!("{HELPER}let again = text;\n");
    fixture(test, &[HELPER, &reuse], |index, _, graph, receipt, _| {
        let (main, helper) = app_modules(graph);
        let owner = BodyOwner::Startup;
        let report = lower_root(receipt, helper, owner, AnalysisLimits::default()).unwrap();
        assert_limits(
            receipt,
            helper,
            owner,
            &report,
            [(2027, 91, 3), (2044, 101, 3)][index],
        );
        if index == 1 {
            assert_reuse(&report, helper, &reuse, "text");
            checked_entry(receipt, main);
        } else {
            assert_eq!(report.completion(), Completion::Checked, "{report:?}");
            let mir = report.mir().unwrap();
            assert!(mir.cancellation().is_none());
            let path = straight_blocks(mir, mir.entry);
            assert_eq!(path.last().unwrap().terminator, Terminator::FinishStartup);
            let ops = path
                .iter()
                .map(|block| &block.operation)
                .filter(|op| !matches!(op, Operation::None))
                .collect::<Vec<_>>();
            let [init, take, cleanup] = ops.as_slice() else {
                panic!("{path:?}")
            };
            assert!(
                matches!(init, Operation::Initialize(place) if place.root.span.bytes() == b"text")
            );
            assert!(
                matches!(take, Operation::Move { from, into: Some(into) } if from.root.span.bytes() == b"text" && into.root.span.bytes() == b"taken")
            );
            assert!(
                matches!(cleanup, Operation::Cleanup(actions) if matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.span.bytes() == b"taken"))
            );
        }
    });
}

fn declaration(module: &loader::Module, kind: DeclarationKind) -> &Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.id.kind == kind)
        .unwrap()
}

#[test]
fn unused_canonical_plain_implementation_is_a_root() {
    let reuse = HELPER.replace(
        "implement unused(value: string): bool { let taken = value;  return true; }",
        "implement unused(value: string): bool { let taken = value; let again = value; return true; }",
    );
    fixture(
        "roots::unused_canonical_plain_implementation_is_a_root",
        &[HELPER, &reuse],
        |index, _, graph, receipt, _| {
            let (main, helper) = app_modules(graph);
            let implementation = declaration(helper, DeclarationKind::Implementation);
            let family = declaration(helper, DeclarationKind::Family);
            let limits = AnalysisLimits::default();
            assert!(matches!(
                lower_body(receipt, helper, &implementation.id, limits),
                Err(QueryError::InvalidRequest)
            ));
            assert!(matches!(
                lower_root(receipt, helper, BodyOwner::Declaration(&family.id), limits),
                Err(QueryError::InvalidRequest)
            ));
            let owner = BodyOwner::Declaration(&implementation.id);
            let report = lower_root(receipt, helper, owner, limits).unwrap();
            assert_limits(
                receipt,
                helper,
                owner,
                &report,
                [(2189, 134, 3), (2154, 116, 3)][index],
            );
            if index == 1 {
                assert_reuse(&report, helper, &reuse, "value");
                checked_entry(receipt, main);
            } else {
                assert_eq!(report.completion(), Completion::Checked, "{report:?}");
                let mir = report.mir().unwrap();
                assert!(mir.cancellation().is_none());
                let ops = linear(mir, mir.entry);
                assert!(
                    matches!(ops[0], Operation::Move { from, into: Some(into) } if from.root.span.bytes() == b"value: string" && implementation.span.start() <= from.root.span.start() && from.root.span.end() <= implementation.span.end() && into.root.span.bytes() == b"taken"),
                    "{ops:?}"
                );
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)] if place.root.span.bytes() == b"taken")
                );
            }
        },
    );
}

#[test]
fn expectation_checks_its_reached_helper_before_publishing_mir() {
    let reuse = HELPER.replace(
        "function retainedControl(value: string): bool { let taken = value;  return true; }",
        "function retainedControl(value: string): bool { let taken = value; let again = value; return true; }",
    );
    fixture(
        "roots::expectation_checks_its_reached_helper_before_publishing_mir",
        &[HELPER, &reuse],
        |index, _, graph, receipt, _| {
            let (main, helper) = app_modules(graph);
            let expectation = declaration(helper, DeclarationKind::Expectation);
            let limits = AnalysisLimits::default();
            assert!(matches!(
                lower_body(receipt, helper, &expectation.id, limits),
                Err(QueryError::InvalidRequest)
            ));
            let owner = BodyOwner::Declaration(&expectation.id);
            let report = lower_root(receipt, helper, owner, limits).unwrap();
            assert_limits(
                receipt,
                helper,
                owner,
                &report,
                [(4168, 205, 3), (4137, 189, 3)][index],
            );
            if index == 1 {
                assert_reuse(&report, helper, &reuse, "value");
                checked_entry(receipt, main);
            } else {
                assert_eq!(report.completion(), Completion::Checked, "{report:?}");
                let callee = named(helper, "retainedControl");
                assert_expectation_call(
                    report.mir().unwrap(),
                    callee,
                    b"retainedControl(\"expectation\")",
                );
            }
            let main_expectation = declaration(main, DeclarationKind::Expectation);
            let ready = named(helper, "ready");
            let independent = lower_root(
                receipt,
                main,
                BodyOwner::Declaration(&main_expectation.id),
                limits,
            )
            .unwrap();
            assert_eq!(
                independent.completion(),
                Completion::Checked,
                "{independent:?}"
            );
            assert_expectation_call(independent.mir().unwrap(), ready, b"ready()");
        },
    );
}

fn assert_expectation_call(mir: &Mir<'_>, callee: &Declaration, expression: &[u8]) {
    assert!(mir.cancellation().is_none());
    let path = straight_blocks(mir, mir.entry);
    let calls: Vec<_> = path
        .iter()
        .filter_map(|block| match block.operation {
            Operation::Call(id) => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(calls, [&callee.id]);
    let end = path.last().unwrap();
    assert_eq!(end.terminator, Terminator::FinishExpectation);
    assert!(
        matches!(end.operation, Operation::Export(place) if place.root.span.bytes() == expression)
    );
}

#[test]
fn root_exits_preserve_cleanup_and_do_not_waive_other_contracts() {
    let source = format!(
        r#"{HELPER}
let tail = "second";
import std::core::dispose as release;
type Custom = {{ keep: string; }}
implement release(value: Custom): void {{ }}
type Decision = {{ allowed: bool; text: string; }}
expect ((Decision {{ allowed: false, text: "scratch" }}).allowed);
"#
    );
    fixture(
        "roots::root_exits_preserve_cleanup_and_do_not_waive_other_contracts",
        &[&source],
        |_, _, graph, receipt, typed| {
            let (main, helper) = app_modules(graph);
            let limits = AnalysisLimits::default();
            let empty = lower_root(receipt, main, BodyOwner::Startup, limits).unwrap();
            let mir = empty.mir().expect("empty startup has a checked exit");
            assert_eq!(mir.blocks.len(), 1);
            assert_eq!(mir.blocks[mir.entry].terminator, Terminator::FinishStartup);

            let startup = lower_root(receipt, helper, BodyOwner::Startup, limits).unwrap();
            let mir = startup.mir().expect("startup has a checked exit");
            let path = straight_blocks(mir, mir.entry);
            assert!(
                matches!(path[path.len() - 2].operation, Operation::Cleanup(ref actions)
            if matches!(actions.as_slice(), [Obligation::Drop(last), Obligation::Drop(first)]
                if last.root.span.bytes() == b"tail" && first.root.span.bytes() == b"taken"))
            );

            let expectation = helper
                .declarations
                .iter()
                .rev()
                .find(|d| d.id.kind == DeclarationKind::Expectation)
                .unwrap();
            let owner = BodyOwner::Declaration(&expectation.id);
            let short = lower_root(
                receipt,
                helper,
                owner,
                AnalysisLimits {
                    max_depth: 4,
                    ..limits
                },
            )
            .unwrap();
            crate::limit::assert_body_limit(&short);
            let result = lower_root(
                receipt,
                helper,
                owner,
                AnalysisLimits {
                    max_depth: 5,
                    ..limits
                },
            )
            .unwrap();
            assert_eq!(result.completion(), Completion::Checked, "{result:?}");
            let mir = result.mir().unwrap();
            let path = straight_blocks(mir, mir.entry);
            let [.., cleanup, exit] = path.as_slice() else {
                panic!("{path:?}")
            };
            assert!(
                matches!(cleanup.operation, Operation::Cleanup(ref actions) if actions.iter().any(|action|
            matches!(action, Obligation::Drop(place) if place.field().unwrap().name.as_deref() == Some("text"))))
            );
            assert_eq!(exit.terminator, Terminator::FinishExpectation);
            assert!(matches!(exit.operation, Operation::Export(_)));

            let dispose = &typed.dispose_role().unwrap().id;
            let disposer = graph
                .implementations()
                .iter()
                .find(|body| body.family == *dispose && body.declaration.module == helper.key)
                .unwrap();
            let provider = typed
                .primitive_providers()
                .iter()
                .find(|provider| {
                    provider.binding.is_some_and(|binding| {
                        binding.operation == compiler::analysis::typed::PrimitiveOperation::CopyText
                    })
                })
                .unwrap()
                .implementation;
            let std_module = graph
                .modules()
                .iter()
                .find(|m| m.key == provider.id.module)
                .unwrap();
            for (module, id) in [(helper, &disposer.declaration), (std_module, &provider.id)] {
                let body = module.declarations.iter().find(|d| d.id == *id).unwrap();
                let result =
                    lower_root(receipt, module, BodyOwner::Declaration(&body.id), limits).unwrap();
                assert_eq!(result.completion(), Completion::Incomplete, "{result:?}");
                assert!(result.mir().is_none() && result.findings().is_empty());
                let blocker = result.blocked_at().unwrap();
                assert_eq!((blocker.module, blocker.node), (&module.key, body.node));
            }
        },
    );
}

#[test]
fn startup_cleanup_blocker_points_to_the_last_startup_statement() {
    let source = format!(
        "{HELPER}\nlet flag = false; let seed = &flag;\nlet mut a: [&bool, bool] = [seed, false]; let mut b: [&bool, bool] = [seed, true];\na[0] = &b[1]; b[0] = &a[1];\nfunction other(): void {{}}\n"
    );
    fixture(
        "roots::startup_cleanup_blocker_points_to_the_last_startup_statement",
        &[&source],
        |_, _, graph, receipt, _| {
            let (_, helper) = app_modules(graph);
            let result = lower_root(
                receipt,
                helper,
                BodyOwner::Startup,
                AnalysisLimits::default(),
            )
            .unwrap();
            assert_eq!(result.completion(), Completion::Incomplete);
            assert!(result.mir().is_none() && result.findings().is_empty());
            assert_eq!(result.blocked_at().unwrap().span.bytes(), b"b[0] = &a[1];");
        },
    );
}

#[test]
fn rejects_foreign_modules_and_body_ids_even_at_zero_budget() {
    fixture(
        "roots::rejects_foreign_modules_and_body_ids_even_at_zero_budget",
        &[HELPER],
        |_, request, graph, receipt, typed| {
            let (main, helper) = app_modules(graph);
            let foreign = loader::load_installed(request.clone()).unwrap();
            let (foreign_main, foreign_helper) = app_modules(&foreign);
            let foreign_expectation = declaration(foreign_helper, DeclarationKind::Expectation);
            let expectation = declaration(helper, DeclarationKind::Expectation);
            let entry = named(main, "entryControl");
            let foreign_entry = named(foreign_main, "entryControl");
            assert_ne!(entry.id.context, foreign_entry.id.context);
            let scalar = &typed
                .primitive_providers()
                .iter()
                .find(|provider| {
                    provider.role == compiler::analysis::typed::PrimitiveRole::Copy
                        && provider.primitive == compiler::analysis::typed::Primitive::Bool
                })
                .unwrap()
                .implementation
                .id;
            let foreign_core = foreign
                .modules()
                .iter()
                .find(|m| m.key == scalar.module)
                .unwrap();
            let zero = AnalysisLimits {
                max_work: 0,
                max_entries: 0,
                max_depth: 0,
            };
            for limits in [zero, AnalysisLimits::default()] {
                for (module, owner) in [
                    (helper, BodyOwner::Declaration(&foreign_expectation.id)),
                    (foreign_helper, BodyOwner::Startup),
                    (foreign_helper, BodyOwner::Declaration(&expectation.id)),
                    (main, BodyOwner::Declaration(&expectation.id)),
                    (foreign_core, BodyOwner::Declaration(scalar)),
                ] {
                    assert!(matches!(
                        lower_root(receipt, module, owner, limits),
                        Err(QueryError::InvalidRequest)
                    ));
                }
                for (module, id) in [(main, &foreign_entry.id), (foreign_main, &entry.id)] {
                    assert!(matches!(
                        lower_body(receipt, module, id, limits),
                        Err(QueryError::InvalidRequest)
                    ));
                }
            }
            let own = lower_root(receipt, helper, BodyOwner::Startup, zero).unwrap();
            crate::limit::assert_body_limit(&own);
        },
    );
}
