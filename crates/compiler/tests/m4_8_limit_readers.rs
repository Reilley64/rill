//! Metering rule 8, round 3: a withheld value must not read as complete. At
//! every cap below a typed run's own, no public reader of the typed report's
//! values (the inventory, the bodies, the conversion report built from the
//! inventory) is more permissive than the same reader at the exact cap, and
//! the limited report never reaches `Checked`, a receipt or (needing a
//! receipt) MIR.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::conversions::{RoleHypothesis, analyze_conversions};
use compiler::analysis::generics::{BodyCompletion, GenericReport, analyze_generics};
use compiler::analysis::ordinary::{
    Completion as OrdinaryCompletion, InputContract::Established, OrdinaryAnalysis, select_inputs,
};
use compiler::analysis::scalars::{BodyOwner, ScalarFailureKind};
use compiler::analysis::typed::{TypedCompletion, TypedReport, analyze_typed};
use compiler::analysis::{AnalysisLimits, Type, TypeTerm};
use compiler::loader::{
    Binding, DeclarationKind, Graph, Limits, Origin, Prelude, Request, Selector, load,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        let f = Self(std::env::temp_dir().join(format!(
            "rill-m4-8-readers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&f.0).unwrap();
        f.write("app/main.rill", source);
        f.write(
            "std/core.rill",
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        );
        f.write(
            "std/convert.rill",
            "export function into<To, From>(value: From): To;",
        );
        f
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn load(&self) -> Graph {
        load(Request {
            entry: self.0.join("app/main.rill"),
            standard_root: self.0.join("std"),
            prelude: Prelude {
                copy: Selector {
                    module: "core.rill".into(),
                    name: "copy".into(),
                },
                dispose: Selector {
                    module: "core.rill".into(),
                    name: "dispose".into(),
                },
            },
            target: "readers-test".into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn named_type<'g>(graph: &'g Graph, name: &str) -> Type<'g> {
    let id = &graph
        .modules()
        .iter()
        .flat_map(|m| &m.declarations)
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
        .id;
    Arc::new(TypeTerm::Nominal {
        declaration: id,
        arguments: Vec::new(),
    })
}

fn standard_export<'g>(graph: &'g Graph, name: &str) -> &'g Binding {
    graph
        .bindings()
        .iter()
        .find(|b| b.public && b.module.origin == Origin::Standard && b.name == name)
        .unwrap()
}

/// How many function and implementation bodies the report reads `Checked`.
fn checked_bodies(graph: &Graph, report: &GenericReport<'_>) -> usize {
    graph
        .modules()
        .iter()
        .flat_map(|module| module.declarations.iter().map(move |d| (module, d)))
        .filter(|(_, d)| {
            matches!(
                d.id.kind,
                DeclarationKind::Function | DeclarationKind::Implementation
            )
        })
        .filter(|(module, d)| {
            report
                .body_status(module, BodyOwner::Declaration(&d.id))
                .is_ok_and(|status| status.completion == BodyCompletion::Checked)
        })
        .count()
}

/// What every public reader of a typed report's values permits. A larger
/// number is more permissive; a limited report must never exceed the exact one.
#[derive(Debug)]
struct Permits {
    /// Typed `Checked`, which grants a receipt.
    typed_checked: usize,
    receipt: usize,
    /// `Checked` bodies in `bodies()`.
    typed_bodies: usize,
    /// `select_inputs(inventory(), ..)` returns a winner or any outcome.
    selection: usize,
    /// `Checked` bodies of `analyze_generics(inventory(), ..)`.
    generics: usize,
    /// `Checked` bodies of `analyze_conversions(inventory(), ..).bodies(..)`.
    conversions: usize,
    /// A `ConversionReport` built over the inventory is not `ResourceLimit`.
    conversion_report_open: usize,
}

fn permits(graph: &Graph, report: &TypedReport<'_>) -> Permits {
    let limits = AnalysisLimits::default();
    let inventory: &OrdinaryAnalysis<'_> = report.inventory();
    let family = &graph.implementations()[0].family;
    let e = named_type(graph, "E");
    let selection = select_inputs(inventory, family, &[Established(e)], &[], limits)
        .map_or(0, |selection| usize::from(selection.outcome.is_some()));
    let binding = standard_export(graph, "into");
    let conversions =
        analyze_conversions(inventory, &[RoleHypothesis::Infallible(binding)], limits)
            .map(|conversion| {
                (
                    usize::from(conversion.completion() != OrdinaryCompletion::ResourceLimit),
                    checked_bodies(graph, &conversion.bodies(limits)),
                )
            })
            .unwrap_or_default();
    Permits {
        typed_checked: usize::from(report.completion() == TypedCompletion::Checked),
        receipt: usize::from(report.receipt().is_some()),
        typed_bodies: checked_bodies(graph, report.bodies()),
        selection,
        generics: checked_bodies(graph, &analyze_generics(inventory, limits)),
        conversions: conversions.1,
        conversion_report_open: conversions.0,
    }
}

fn dominated(limited: &Permits, exact: &Permits) -> bool {
    limited.typed_checked <= exact.typed_checked
        && limited.receipt <= exact.receipt
        && limited.typed_bodies <= exact.typed_bodies
        && limited.selection <= exact.selection
        && limited.generics <= exact.generics
        && limited.conversions <= exact.conversions
        && limited.conversion_report_open <= exact.conversion_report_open
}

const TAIL: &str = "enum E { Left, Right } function g(): bool { return true; } \
    function f<A>(a: A): bool; implement f(a: E): bool { return true; }";

/// Every work cap from zero to one short, one short on entries and one short
/// on depth: a limited typed report never reaches `Checked`, a receipt or MIR
/// (`lower_body`, `lower_root` and `analyze_ownership` take a `TypedReceipt`),
/// and no reader of its values is more permissive than at the exact cap.
#[track_caller]
fn sweep(source: &str) {
    let source = format!("import std::convert::into; {source} {TAIL}");
    let fixture = Fixture::new(&source);
    let graph = fixture.load();
    let full = analyze_typed(&graph, AnalysisLimits::default());
    let stats = full.statistics();
    let exact_permits = permits(&graph, &full);
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let mut caps = (0..stats.work)
        .map(|max_work| AnalysisLimits { max_work, ..exact })
        .collect::<Vec<_>>();
    caps.push(AnalysisLimits {
        max_entries: exact.max_entries - 1,
        ..exact
    });
    caps.push(AnalysisLimits {
        max_depth: exact.max_depth - 1,
        ..exact
    });
    let mut limited = 0;
    for cap in caps {
        let report = analyze_typed(&graph, cap);
        if report.completion() != TypedCompletion::ResourceLimit {
            continue;
        }
        limited += 1;
        assert!(report.receipt().is_none(), "a receipt at {cap:?}");
        // Each withheld value reads as withheld, not as complete.
        assert_eq!(
            report.inventory().completion(),
            OrdinaryCompletion::ResourceLimit,
            "inventory at {cap:?}"
        );
        assert_eq!(
            report.bodies().completion(),
            ScalarFailureKind::ResourceLimit,
            "bodies at {cap:?}"
        );
        let found = permits(&graph, &report);
        assert!(
            dominated(&found, &exact_permits),
            "at {cap:?}: {found:?} is more permissive than the exact cap {exact_permits:?}"
        );
    }
    assert!(limited > 0, "the sweep saw no limited report");
}

#[test]
fn limited_typed_reports_are_never_more_permissive_with_a_missing_type() {
    sweep("type Gone = { x: Missing };");
}

#[test]
fn limited_typed_reports_are_never_more_permissive_with_a_duplicate_field() {
    sweep("type P = { x: bool; x: bool };");
}

#[test]
fn limited_typed_reports_are_never_more_permissive_with_a_self_recursive_type() {
    sweep("type Loop = { a: Loop };");
}

#[test]
fn limited_typed_reports_are_never_more_permissive_with_a_missing_and_a_recursive_type() {
    sweep("type Gone = { x: Missing }; type Loop = { a: Loop };");
}

#[test]
fn a_limited_typed_inventory_reads_resource_limit() {
    let fixture = Fixture::new(&format!(
        "import std::convert::into; type Gone = {{ x: Missing }}; {TAIL}"
    ));
    let graph = fixture.load();
    let full = analyze_typed(&graph, AnalysisLimits::default());
    let stats = full.statistics();
    let report = analyze_typed(
        &graph,
        AnalysisLimits {
            max_work: stats.work - 1,
            max_entries: stats.entries,
            max_depth: stats.max_depth,
        },
    );
    assert_eq!(report.completion(), TypedCompletion::ResourceLimit);
    assert_eq!(
        report.inventory().completion(),
        OrdinaryCompletion::ResourceLimit
    );
    limit::assert_typed_limit(&report);
}
