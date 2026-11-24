//! The one place that says what `ResourceLimit` publishes (metering rule 8):
//! no MIR and no finding (a typed report: no receipt and no finding in any list
//! it publishes; an ordinary, generics or conversion report: no finding in any
//! list it publishes).
//! Every `ResourceLimit` assertion calls one of these, so a site cannot omit
//! part of it. The `*_flaws` forms list what is wrong, for tests that collect
//! failures instead of panicking.
#![allow(dead_code)]

use compiler::analysis::conversions::ConversionReport;
use compiler::analysis::generics::GenericReport;
use compiler::analysis::ordinary::{Completion as OrdinaryCompletion, OrdinaryAnalysis};
use compiler::analysis::ownership::{Completion, GraphCompletion, GraphReport, Report};
use compiler::analysis::scalars::ScalarFailureKind;
use compiler::analysis::typed::{TypedCompletion, TypedReport};
use compiler::loader::Graph;

/// A whole-graph report: no finding, and no ordinary MIR in any root or active disposer.
pub(crate) fn graph_limit_flaws(report: &GraphReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != GraphCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if !report.findings().is_empty() {
        flaws.push("publishes a finding");
    }
    if report
        .roots()
        .iter()
        .chain(report.active_disposers())
        .any(|root| root.mir().is_some() || root.ordinary_mir().is_some())
    {
        flaws.push("publishes MIR");
    }
    flaws
}

/// A targeted `lower_body` or `lower_root` report: no finding and no MIR.
pub(crate) fn body_limit_flaws(report: &Report<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != Completion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if !report.findings().is_empty() {
        flaws.push("publishes a finding");
    }
    if report.mir().is_some() {
        flaws.push("publishes MIR");
    }
    flaws
}

/// A typed report: no receipt, and no finding in any list it publishes: body,
/// conditional, annotation, public-interface, declaration issue, and the
/// inventory's type findings and declaration issues.
pub(crate) fn typed_limit_flaws(report: &TypedReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != TypedCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.receipt().is_some() {
        flaws.push("grants a receipt");
    }
    let bodies = report.bodies();
    if !bodies.findings().is_empty() {
        flaws.push("publishes a body finding");
    }
    if !bodies.conditional_findings().is_empty() {
        flaws.push("publishes a conditional finding");
    }
    if bodies.annotations().findings().next().is_some() {
        flaws.push("publishes an annotation finding");
    }
    if !bodies.declaration_issues().is_empty() {
        flaws.push("publishes a declaration issue");
    }
    if !report.public_interface_findings().is_empty() {
        flaws.push("publishes a public-interface finding");
    }
    let inventory = report.inventory();
    if inventory.type_facts().findings().next().is_some() {
        flaws.push("publishes an inventory type finding");
    }
    if !inventory.declaration_issues().is_empty() {
        flaws.push("publishes an inventory declaration issue");
    }
    flaws
}

/// An `analyze_ordinary` report: no finding in the type facts, the declaration
/// issues or any declaration header's metadata. The graph names the headers.
pub(crate) fn ordinary_limit_flaws(
    graph: &Graph,
    report: &OrdinaryAnalysis<'_>,
) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != OrdinaryCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.type_facts().findings().next().is_some() {
        flaws.push("publishes a type finding");
    }
    if !report.declaration_issues().is_empty() {
        flaws.push("publishes a declaration issue");
    }
    if header_finding(graph, report) {
        flaws.push("publishes a header metadata finding");
    }
    flaws
}

/// Whether any declaration header of the report publishes a metadata finding.
pub(crate) fn header_finding(graph: &Graph, report: &OrdinaryAnalysis<'_>) -> bool {
    graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .any(|declaration| {
            report
                .header_metadata(&declaration.id)
                .unwrap()
                .is_some_and(|header| header.type_metadata().findings().next().is_some())
        })
}

/// An `analyze_generics` report (or `ConversionReport::bodies`): no finding in
/// any list it publishes: body, conditional, declaration issue and annotation.
pub(crate) fn generics_limit_flaws(report: &GenericReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != ScalarFailureKind::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if !report.findings().is_empty() {
        flaws.push("publishes a body finding");
    }
    if !report.conditional_findings().is_empty() {
        flaws.push("publishes a conditional finding");
    }
    if !report.declaration_issues().is_empty() {
        flaws.push("publishes a declaration issue");
    }
    if report.annotations().findings().next().is_some() {
        flaws.push("publishes an annotation finding");
    }
    flaws
}

/// A `ConversionReport`: no finding, no declaration, use or obligation through
/// `type_facts` and no header through `header_metadata` (metering rule 8). Its
/// bodies are `assert_generics_limit`'s. The graph names the headers.
pub(crate) fn conversion_limit_flaws(
    graph: &Graph,
    report: &ConversionReport<'_, '_>,
) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != OrdinaryCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.type_facts().findings().next().is_some() {
        flaws.push("publishes a type finding");
    }
    let facts = report.type_facts();
    if facts.declarations().next().is_some()
        || facts.type_uses().next().is_some()
        || facts.obligations().next().is_some()
    {
        flaws.push("publishes type facts");
    }
    if conversion_header_finding(graph, report) {
        flaws.push("publishes a header metadata finding");
    }
    if conversion_headers(graph, report) != 0 {
        flaws.push("publishes a header");
    }
    flaws
}

/// How many declarations of the graph have a header through the conversion
/// report's `header_metadata`. `Ok(None)` is both "no header" and, at
/// `ResourceLimit`, "withheld".
pub(crate) fn conversion_headers(graph: &Graph, report: &ConversionReport<'_, '_>) -> usize {
    graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .filter(|declaration| report.header_metadata(&declaration.id).unwrap().is_some())
        .count()
}

/// Whether any declaration header of the conversion report publishes a metadata finding.
pub(crate) fn conversion_header_finding(graph: &Graph, report: &ConversionReport<'_, '_>) -> bool {
    graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .any(|declaration| {
            report
                .header_metadata(&declaration.id)
                .unwrap()
                .is_some_and(|header| header.type_metadata().findings().next().is_some())
        })
}

#[track_caller]
pub(crate) fn assert_graph_limit(report: &GraphReport<'_>) {
    let flaws = graph_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_body_limit(report: &Report<'_>) {
    let flaws = body_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_typed_limit(report: &TypedReport<'_>) {
    let flaws = typed_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_ordinary_limit(graph: &Graph, report: &OrdinaryAnalysis<'_>) {
    let flaws = ordinary_limit_flaws(graph, report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_conversion_limit(graph: &Graph, report: &ConversionReport<'_, '_>) {
    let flaws = conversion_limit_flaws(graph, report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_generics_limit(report: &GenericReport<'_>) {
    let flaws = generics_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}
