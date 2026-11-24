//! The one place that says what `ResourceLimit` publishes (metering rule 8):
//! no MIR and no finding (a typed report: no receipt and no finding in any list
//! it publishes).
//! Every `ResourceLimit` assertion calls one of these, so a site cannot omit
//! part of it. The `*_flaws` forms list what is wrong, for tests that collect
//! failures instead of panicking.
#![allow(dead_code)]

use compiler::analysis::ownership::{Completion, GraphCompletion, GraphReport, Report};
use compiler::analysis::typed::{TypedCompletion, TypedReport};

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
